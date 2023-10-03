use const_lru::ConstLru;
use http::{
    header::{ETAG, IF_NONE_MATCH, LAST_MODIFIED, VARY},
    HeaderMap, HeaderValue,
};
use http_body::Body;
use num_traits::{PrimInt, Unsigned};
use std::{alloc::alloc, alloc::Layout, error::Error, ptr::addr_of_mut, time::SystemTime};
use time::{format_description::well_known::Rfc2822, OffsetDateTime};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::PollSender;

use crate::{
    base64_blake3_body_etag::base64_blake3_body_etag,
    simple_cache_key::{calc_simple_etag_cache_key, SimpleEtagCacheKey},
    CacheGetResponse, CacheGetResponseResult, CacheProvider,
};

mod err;
mod get;
mod put;
mod tres_body;

pub use err::*;
pub use get::*;
pub use put::*;
pub use tres_body::*;

pub type ConstLruProviderCacheKey = SimpleEtagCacheKey;

/// Tuple containing the request to the provider and the oneshot
/// sender for the provider to send the response to
pub type ReqTup<ReqBody, ResBody> = (
    ConstLruProviderReq<ReqBody, ResBody>,
    oneshot::Sender<Result<ConstLruProviderRes<ReqBody>, ConstLruProviderError>>,
);

#[derive(Debug)]
pub enum ConstLruProviderReq<ReqBody, ResBody> {
    Get(http::Request<ReqBody>),
    Put(ConstLruProviderCacheKey, http::Response<ResBody>),
}

#[derive(Debug)]
pub enum ConstLruProviderRes<ReqBody> {
    Get(CacheGetResponse<ReqBody, ConstLruProviderCacheKey>),
    Put(http::Response<ConstLruProviderTResBody>),
}

/// A basic in-memory ConstLru-backed cache provider.
///
/// Meant to be a singleton communicated with using channels via [`ConstLruProviderHandle`]
///
/// Uses [`SimpleEtagCacheKey`] as key type.
///
/// Stores the SystemTime of when the cache entry was created, which also serves as the response's
/// last-modified header value
///
/// Passthroughs responses that already have ETag or Vary headers set.
///
/// Typical type args for use in axum:
///
/// ReqBody = hyper::body::Body
/// ResBody = axum::body::BoxBody
pub struct ConstLruProvider<ReqBody, ResBody, const CAP: usize, I: PrimInt + Unsigned = usize> {
    const_lru: ConstLru<ConstLruProviderCacheKey, (String, SystemTime), CAP, I>,
    req_rx: mpsc::Receiver<ReqTup<ReqBody, ResBody>>,
}

impl<
        ReqBody: Send + 'static,
        ResBody: Send + Body + 'static,
        const CAP: usize,
        I: PrimInt + Unsigned + Send + 'static,
    > ConstLruProvider<ReqBody, ResBody, CAP, I>
where
    <ResBody as Body>::Data: Send,
    <ResBody as Body>::Error: Error + Send + Sync,
{
    /// Creates a ConstLruProvider and returns the handle to it.
    /// ConstLruProvider is dropped once all handles are dropped.
    /// Should be called once on server init
    pub fn init(req_buffer: usize) -> ConstLruProviderHandle<ReqBody, ResBody> {
        let (req_tx, req_rx) = mpsc::channel(req_buffer);

        let mut this = Self::boxed(req_rx);
        tokio::spawn(async move { this.run().await });

        ConstLruProviderHandle {
            req_tx: PollSender::new(req_tx),
        }
    }

    fn boxed(req_rx: mpsc::Receiver<ReqTup<ReqBody, ResBody>>) -> Box<Self> {
        unsafe {
            let ptr = alloc(Layout::new::<Self>()) as *mut Self;
            let const_lru_ptr = addr_of_mut!((*ptr).const_lru);
            ConstLru::init_at_alloc(const_lru_ptr);
            let req_rx_ptr = addr_of_mut!((*ptr).req_rx);
            req_rx_ptr.write(req_rx);
            Box::from_raw(ptr)
        }
    }

    /// long-running loop
    async fn run(&mut self) {
        while let Some((req, resp_tx)) = self.req_rx.recv().await {
            let res = match req {
                ConstLruProviderReq::Get(req) => {
                    self.on_get_request(req).map(ConstLruProviderRes::Get)
                }
                ConstLruProviderReq::Put(key, resp) => self
                    .on_put_request(key, resp)
                    .await
                    .map(ConstLruProviderRes::Put),
            };
            // ignore if resp_rx dropped
            let _ = resp_tx.send(res);
        }
        // exits when all req_tx dropped
    }

    fn on_get_request(
        &mut self,
        req: http::Request<ReqBody>,
    ) -> Result<CacheGetResponse<ReqBody, ConstLruProviderCacheKey>, ConstLruProviderError> {
        let key = calc_simple_etag_cache_key(&req);
        let (cache_etag, last_modified) = match self.const_lru.get(&key) {
            Some(e) => e,
            None => {
                return Ok(CacheGetResponse {
                    req,
                    result: crate::CacheGetResponseResult::Miss(key),
                })
            }
        };
        let if_none_match_iter = req.headers().get_all(IF_NONE_MATCH);
        for etag in if_none_match_iter {
            let etag_str = match etag.to_str() {
                Ok(s) => s,
                Err(_) => continue,
            };
            if etag_str == cache_etag {
                let mut header_map = HeaderMap::new();
                Self::set_response_headers(&mut header_map, etag.clone(), *last_modified);
                return Ok(CacheGetResponse {
                    req,
                    result: CacheGetResponseResult::Hit(header_map),
                });
            }
        }
        Ok(CacheGetResponse {
            req,
            result: CacheGetResponseResult::Miss(key),
        })
    }

    async fn on_put_request(
        &mut self,
        key: ConstLruProviderCacheKey,
        resp: http::Response<ResBody>,
    ) -> Result<http::Response<ConstLruProviderTResBody>, ConstLruProviderError> {
        let (mut parts, body) = resp.into_parts();
        // TODO: want to use hyper::body::aggregate() instead
        // but idk how to do it without consuming the impl Buf
        let body_bytes = hyper::body::to_bytes(body)
            .await
            .map_err(|e| ConstLruProviderError::ReadResBody(e.into()))?;

        if Self::should_passthrough(&parts) {
            return Ok(http::Response::from_parts(parts, body_bytes.into()));
        }

        let etag = base64_blake3_body_etag(&body_bytes);
        let last_modified = SystemTime::now();
        self.const_lru
            .insert(key, (etag.to_str().unwrap().to_owned(), last_modified));
        Self::set_response_headers(&mut parts.headers, etag, last_modified);

        Ok(http::Response::from_parts(parts, body_bytes.into()))
    }

    fn should_passthrough(parts: &http::response::Parts) -> bool {
        let headers = &parts.headers;
        headers.contains_key(VARY) || headers.contains_key(ETAG)
    }

    fn set_response_headers(
        headers_mut: &mut HeaderMap,
        etag_val: HeaderValue,
        last_modified_val: SystemTime,
    ) {
        headers_mut.append(ETAG, etag_val);
        let last_modified_val = OffsetDateTime::from(last_modified_val)
            .format(&Rfc2822)
            .unwrap();
        headers_mut.append(
            LAST_MODIFIED,
            HeaderValue::from_str(&last_modified_val).unwrap(),
        );
        SimpleEtagCacheKey::set_response_headers(headers_mut);
    }
}

// SERVICE HANDLE

pub struct ConstLruProviderHandle<ReqBody, ResBody> {
    req_tx: PollSender<ReqTup<ReqBody, ResBody>>,
}

impl<ReqBody, ResBody> Clone for ConstLruProviderHandle<ReqBody, ResBody> {
    fn clone(&self) -> Self {
        Self {
            req_tx: self.req_tx.clone(),
        }
    }
}

impl<ReqBody: Send, ResBody: Send> CacheProvider<ReqBody, ResBody>
    for ConstLruProviderHandle<ReqBody, ResBody>
{
    type Key = ConstLruProviderCacheKey;
    type TResBody = ConstLruProviderTResBody;
}
