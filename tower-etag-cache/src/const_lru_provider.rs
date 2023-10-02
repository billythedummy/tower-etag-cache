use bytes::Bytes;
use const_lru::ConstLru;
use http::{HeaderMap, HeaderValue};
use http_body::Body;
use num_traits::{PrimInt, Unsigned};
use pin_project::pin_project;
use std::{
    alloc::alloc,
    alloc::Layout,
    convert::Infallible,
    fmt::Display,
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
    CacheGetResponse, CacheGetResponseResult, CacheProvider,
};

pub type ReqTup<ReqBody, ResBody> = (
    ConstLruProviderReq<ReqBody, ResBody>,
    oneshot::Sender<ConstLruProviderRes<ReqBody>>,
);

#[derive(Debug)]
pub enum ConstLruProviderReq<ReqBody, ResBody> {
    Get(http::Request<ReqBody>),
    Put(String, http::Response<ResBody>),
}

#[derive(Debug)]
#[pin_project]
pub struct ConstLruProviderTResBody(Bytes);

impl From<Bytes> for ConstLruProviderTResBody {
    fn from(value: Bytes) -> Self {
        Self(value)
    }
}

impl Body for ConstLruProviderTResBody {
    type Data = Bytes;

    type Error = Infallible;

    fn poll_data(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Self::Data, Self::Error>>> {
        Poll::Ready(Some(Ok(std::mem::take(self.project().0))))
    }

    fn poll_trailers(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Result<Option<HeaderMap>, Self::Error>> {
        Poll::Ready(Ok(None))
    }
}

#[derive(Debug)]
pub enum ConstLruProviderRes<ReqBody> {
    Get(CacheGetResponse<ReqBody, String>),
    Put(http::Response<ConstLruProviderTResBody>),
}

/// 
/// ReqBody = axum::body::Body
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

    fn on_get_request(&mut self, req: http::Request<ReqBody>) -> CacheGetResponse<ReqBody, String> {
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
    ) -> http::Response<ConstLruProviderTResBody> {
        let (mut parts, body) = resp.into_parts();
        // TODO: use aggregate() instead
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

// GET

#[pin_project]
pub struct ConstLruProviderGetFuture<ReqBody> {
    #[pin]
    resp_rx: oneshot::Receiver<ConstLruProviderRes<ReqBody>>,
}

impl<ReqBody> Future for ConstLruProviderGetFuture<ReqBody> {
    type Output = Result<CacheGetResponse<ReqBody, String>, ConstLruProviderError>;

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
        oneshot::Sender<ConstLruProviderRes<ReqBody>>,
    ): Send,
{
    type Response = CacheGetResponse<ReqBody, String>;

    type Error = ConstLruProviderError;

    type Future = ConstLruProviderGetFuture<ReqBody>;

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
pub struct ConstLruProviderPutFuture<ReqBody> {
    #[pin]
    resp_rx: oneshot::Receiver<ConstLruProviderRes<ReqBody>>,
}

impl<ReqBody> Future for ConstLruProviderPutFuture<ReqBody> {
    type Output = Result<http::Response<ConstLruProviderTResBody>, ConstLruProviderError>;

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
        oneshot::Sender<ConstLruProviderRes<ReqBody>>,
    ): Send,
{
    type Response = http::Response<ConstLruProviderTResBody>;

    type Error = ConstLruProviderError;

    type Future = ConstLruProviderPutFuture<ReqBody>;

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

impl<ReqBody: Send, ResBody: Send> CacheProvider<ReqBody, ResBody>
    for ConstLruProviderHandle<ReqBody, ResBody>
{
    type Key = String;
    type TResBody = ConstLruProviderTResBody;
}

// ERROR

// Error type must implement std::Error else axum will throw
// `the trait bound HandleError<...> is not satisfied`

#[derive(Debug, Clone)]
pub enum ConstLruProviderError {
    OneshotRecv(RecvError),
    MpscSend,
}

impl Display for ConstLruProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OneshotRecv(e) => e.fmt(f),
            Self::MpscSend => write!(f, "MpscSend"),
        }
    }
}
