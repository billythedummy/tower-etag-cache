use const_lru::ConstLru;
use http::{HeaderMap, HeaderValue};
use http_body::Body;
use hyper::body::Bytes;
use num_traits::{PrimInt, Unsigned};
use pin_project::pin_project;
use std::{
    alloc::alloc,
    alloc::Layout,
    future::Future,
    pin::Pin,
    ptr::addr_of_mut,
    task::{Context, Poll},
    time::SystemTime,
};
use time::{format_description::well_known::Rfc2822, OffsetDateTime};
use tokio::sync::{
    mpsc,
    oneshot::{self, error::RecvError},
};
use tokio_util::sync::PollSender;
use tower_service::Service;

use crate::{
    base64_blake3_body_etag::base64_blake3_body_etag, simple_cache_key::simple_etag_cache_key,
    CacheGetProvider, CacheGetResponse, CacheGetResponseResult, CacheProvider, CachePutProvider,
};

#[derive(Debug)]
pub enum ConstLruProviderReq<ReqBody, ResBody> {
    Get(http::Request<ReqBody>),
    Put(String, http::Response<ResBody>),
}

#[derive(Debug)]
pub enum ConstLruProviderRes<ReqBody, ResBody> {
    Get(CacheGetResponse<http::Request<ReqBody>, String>),
    Put(http::Response<ResBody>),
}

pub struct ConstLruProvider<ReqBody, ResBody, const CAP: usize, I: PrimInt + Unsigned = usize> {
    const_lru: ConstLru<String, (String, SystemTime), CAP, I>,
    req_rx: mpsc::Receiver<(
        ConstLruProviderReq<ReqBody, ResBody>,
        oneshot::Sender<ConstLruProviderRes<ReqBody, ResBody>>,
    )>,
}

impl<
        ReqBody: Send + 'static,
        ResBody: Send + Body + From<Bytes> + 'static,
        const CAP: usize,
        I: PrimInt + Unsigned + Send + 'static,
    > ConstLruProvider<ReqBody, ResBody, CAP, I>
where
    <ResBody as Body>::Data: Send,
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

    fn boxed(
        req_rx: mpsc::Receiver<(
            ConstLruProviderReq<ReqBody, ResBody>,
            oneshot::Sender<ConstLruProviderRes<ReqBody, ResBody>>,
        )>,
    ) -> Box<Self> {
        unsafe {
            let ptr = alloc(Layout::new::<Self>()) as *mut Self;
            let const_lru_ptr = addr_of_mut!((*ptr).const_lru);
            ConstLru::init_at_alloc(const_lru_ptr);
            (*ptr).req_rx = req_rx;
            Box::from_raw(ptr)
        }
    }

    async fn run(&mut self) {
        while let Some((req, resp_tx)) = self.req_rx.recv().await {
            let res = match req {
                ConstLruProviderReq::Get(req) => ConstLruProviderRes::Get(self.on_get_request(req)),
                ConstLruProviderReq::Put(key, resp) => {
                    ConstLruProviderRes::Put(self.on_put_request(key, resp).await)
                }
            };
            // ignore if resp_rx dropped
            let _ = resp_tx.send(res);
        }
        // exits when all req_tx dropped
    }

    fn on_get_request(
        &mut self,
        req: http::Request<ReqBody>,
    ) -> CacheGetResponse<http::Request<ReqBody>, String> {
        let key = simple_etag_cache_key(&req).unwrap(); // TODO: handle error
        let (cache_etag, last_modified) = match self.const_lru.get(&key) {
            Some(e) => e,
            None => {
                return CacheGetResponse {
                    req,
                    result: crate::CacheGetResponseResult::Miss(key),
                }
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
                return CacheGetResponse {
                    req,
                    result: CacheGetResponseResult::Hit(header_map),
                };
            }
        }
        CacheGetResponse {
            req,
            result: CacheGetResponseResult::Miss(key),
        }
    }

    async fn on_put_request(
        &mut self,
        key: String,
        resp: http::Response<ResBody>,
    ) -> http::Response<ResBody> {
        let (mut parts, body) = resp.into_parts();
        let body_bytes = match hyper::body::to_bytes(body).await {
            Ok(b) => b,
            Err(_) => unreachable!(), // TODO: handle error
        };
        let etag = base64_blake3_body_etag(body_bytes.iter());
        self.const_lru
            .insert(key, (etag.to_str().unwrap().to_owned(), SystemTime::now()));
        parts.headers.append("etag", etag);

        http::Response::from_parts(parts, body_bytes.into())
    }
}

// SERVICE HANDLE

#[derive(Clone)]
pub struct ConstLruProviderHandle<ReqBody, ResBody> {
    req_tx: PollSender<(
        ConstLruProviderReq<ReqBody, ResBody>,
        oneshot::Sender<ConstLruProviderRes<ReqBody, ResBody>>,
    )>,
}

// GET

#[pin_project]
pub struct ConstLruProviderGetFuture<ReqBody, ResBody> {
    #[pin]
    resp_rx: oneshot::Receiver<ConstLruProviderRes<ReqBody, ResBody>>,
}

impl<ReqBody, ResBody> Future for ConstLruProviderGetFuture<ReqBody, ResBody> {
    type Output = Result<CacheGetResponse<http::Request<ReqBody>, String>, ConstLruProviderError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.project()
            .resp_rx
            .poll(cx)
            .map(|result| {
                result.map(|en| match en {
                    ConstLruProviderRes::Get(r) => r,
                    _ => unreachable!(),
                })
            })
            .map_err(ConstLruProviderError::OneshotRecv)
    }
}

impl<ReqBody, ResBody> Service<http::Request<ReqBody>> for ConstLruProviderHandle<ReqBody, ResBody>
where
    (
        ConstLruProviderReq<ReqBody, ResBody>,
        oneshot::Sender<ConstLruProviderRes<ReqBody, ResBody>>,
    ): Send,
{
    type Response = CacheGetResponse<http::Request<ReqBody>, String>;

    type Error = ConstLruProviderError;

    type Future = ConstLruProviderGetFuture<ReqBody, ResBody>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.req_tx
            .poll_reserve(cx)
            .map_err(|_| ConstLruProviderError::MpscSend)
    }

    fn call(&mut self, req: http::Request<ReqBody>) -> Self::Future {
        let (resp_tx, resp_rx) = oneshot::channel();
        // safe to ignore err since resp_tx will be dropped
        // here and next poll will fail
        let _ = self
            .req_tx
            .send_item((ConstLruProviderReq::Get(req), resp_tx));
        ConstLruProviderGetFuture { resp_rx }
    }
}

// PUT

#[pin_project]
pub struct ConstLruProviderPutFuture<ReqBody, ResBody> {
    #[pin]
    resp_rx: oneshot::Receiver<ConstLruProviderRes<ReqBody, ResBody>>,
}

impl<ReqBody, ResBody> Future for ConstLruProviderPutFuture<ReqBody, ResBody> {
    type Output = Result<http::Response<ResBody>, ConstLruProviderError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.project()
            .resp_rx
            .poll(cx)
            .map(|result| {
                result.map(|en| match en {
                    ConstLruProviderRes::Put(r) => r,
                    _ => unreachable!(),
                })
            })
            .map_err(ConstLruProviderError::OneshotRecv)
    }
}

impl<ReqBody, ResBody> Service<(String, http::Response<ResBody>)>
    for ConstLruProviderHandle<ReqBody, ResBody>
where
    (
        ConstLruProviderReq<ReqBody, ResBody>,
        oneshot::Sender<ConstLruProviderRes<ReqBody, ResBody>>,
    ): Send,
{
    type Response = http::Response<ResBody>;

    type Error = ConstLruProviderError;

    type Future = ConstLruProviderPutFuture<ReqBody, ResBody>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.req_tx
            .poll_reserve(cx)
            .map_err(|_| ConstLruProviderError::MpscSend)
    }

    fn call(&mut self, (key, resp): (String, http::Response<ResBody>)) -> Self::Future {
        let (resp_tx, resp_rx) = oneshot::channel();
        // safe to ignore err since resp_tx will be dropped
        // here and next poll will fail
        let _ = self
            .req_tx
            .send_item((ConstLruProviderReq::Put(key, resp), resp_tx));
        ConstLruProviderPutFuture { resp_rx }
    }
}

// IMPLS

impl<ReqBody: Send, ResBody: Send> CacheGetProvider<http::Request<ReqBody>>
    for ConstLruProviderHandle<ReqBody, ResBody>
{
    type Key = String;
}

impl<ReqBody: Send, ResBody: Send> CachePutProvider<http::Response<ResBody>>
    for ConstLruProviderHandle<ReqBody, ResBody>
{
    type Key = String;
}

impl<ReqBody: Send, ResBody: Send> CacheProvider<http::Request<ReqBody>, http::Response<ResBody>>
    for ConstLruProviderHandle<ReqBody, ResBody>
{
    type K = String;
}

// ERROR

pub enum ConstLruProviderError {
    OneshotRecv(RecvError),
    MpscSend,
}
