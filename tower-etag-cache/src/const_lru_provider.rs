use const_lru::ConstLru;
use num_traits::{PrimInt, Unsigned};
use pin_project::pin_project;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::SystemTime,
};
use tokio::sync::{
    mpsc,
    oneshot::{self, error::RecvError},
};
use tokio_util::sync::PollSender;
use tower_service::Service;

use crate::CacheGetResponse;

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

// SERVICE

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

// ERROR

pub enum ConstLruProviderError {
    OneshotRecv(RecvError),
    MpscSend,
}
