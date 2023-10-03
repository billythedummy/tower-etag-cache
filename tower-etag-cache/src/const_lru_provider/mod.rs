use const_lru::ConstLru;
use http::{HeaderMap, HeaderValue};
use http_body::Body;
use num_traits::{PrimInt, Unsigned};
use std::{alloc::alloc, alloc::Layout, error::Error, ptr::addr_of_mut, time::SystemTime};
use time::{format_description::well_known::Rfc2822, OffsetDateTime};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::PollSender;

use crate::{
    base64_blake3_body_etag::base64_blake3_body_etag, simple_cache_key::simple_etag_cache_key,
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

/// Tuple containing the request to the provider and the oneshot
/// sender for the provider to send the response to
pub type ReqTup<ReqBody, ResBody> = (
    ConstLruProviderReq<ReqBody, ResBody>,
    oneshot::Sender<Result<ConstLruProviderRes<ReqBody>, ConstLruProviderError>>,
);

#[derive(Debug)]
pub enum ConstLruProviderReq<ReqBody, ResBody> {
    Get(http::Request<ReqBody>),
    Put(String, http::Response<ResBody>),
}

#[derive(Debug)]
pub enum ConstLruProviderRes<ReqBody> {
    Get(CacheGetResponse<ReqBody, String>),
    Put(http::Response<ConstLruProviderTResBody>),
}

///
/// ReqBody = hyper::body::Body
/// ResBody = axum::body::BoxBody
pub struct ConstLruProvider<ReqBody, ResBody, const CAP: usize, I: PrimInt + Unsigned = usize> {
    const_lru: ConstLru<String, (String, SystemTime), CAP, I>,
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
    ) -> Result<CacheGetResponse<ReqBody, String>, ConstLruProviderError> {
        let key = simple_etag_cache_key(&req).unwrap(); // TODO: handle error
        let (cache_etag, last_modified) = match self.const_lru.get(&key) {
            Some(e) => e,
            None => {
                return Ok(CacheGetResponse {
                    req,
                    result: crate::CacheGetResponseResult::Miss(key),
                })
            }
        };
        let if_none_match_iter = req.headers().get_all("if-none-match");
        for etag in if_none_match_iter {
            let etag_str = match etag.to_str() {
                Ok(s) => s,
                Err(_) => continue,
            };
            if etag_str == cache_etag {
                let mut header_map = HeaderMap::new();
                header_map.append("etag", etag.clone());
                let last_modified_val = OffsetDateTime::from(*last_modified)
                    .format(&Rfc2822)
                    .unwrap();
                header_map.append(
                    "last-modified",
                    HeaderValue::from_str(&last_modified_val).unwrap(),
                );
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
        key: String,
        resp: http::Response<ResBody>,
    ) -> Result<http::Response<ConstLruProviderTResBody>, ConstLruProviderError> {
        let (mut parts, body) = resp.into_parts();
        // TODO: use aggregate() instead
        let body_bytes = hyper::body::to_bytes(body)
            .await
            .map_err(|e| ConstLruProviderError::ReadResBody(e.into()))?;
        let etag = base64_blake3_body_etag(body_bytes.iter());
        self.const_lru
            .insert(key, (etag.to_str().unwrap().to_owned(), SystemTime::now()));
        parts.headers.append("etag", etag);

        Ok(http::Response::from_parts(parts, body_bytes.into()))
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
    type Key = String;
    type TResBody = ConstLruProviderTResBody;
}
