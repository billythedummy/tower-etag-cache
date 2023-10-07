use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use http_body::Body;
use pin_project::pin_project;
use tokio::sync::oneshot;
use tower_service::Service;

use crate::CacheGetResponse;

use super::{
    err::ConstLruProviderError, ConstLruProviderCacheKey, ConstLruProviderHandle,
    ConstLruProviderReq, ConstLruProviderRes, ReqTup,
};

#[pin_project]
pub struct ConstLruProviderGetFuture<ReqBody, ResBody: Body> {
    #[pin]
    resp_rx: oneshot::Receiver<
        Result<ConstLruProviderRes<ReqBody>, ConstLruProviderError<ResBody::Error>>,
    >,
}

impl<ReqBody, ResBody: Body> Future for ConstLruProviderGetFuture<ReqBody, ResBody> {
    type Output = Result<
        CacheGetResponse<ReqBody, ConstLruProviderCacheKey>,
        ConstLruProviderError<ResBody::Error>,
    >;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.project().resp_rx.poll(cx).map(|oneshot_result| {
            oneshot_result.map_or_else(
                |e| Err(ConstLruProviderError::OneshotRecv(e)),
                |result| {
                    result.map(|en| match en {
                        ConstLruProviderRes::Get(r) => r,
                        _ => unreachable!(),
                    })
                },
            )
        })
    }
}

impl<ReqBody, ResBody: Body> Service<http::Request<ReqBody>>
    for ConstLruProviderHandle<ReqBody, ResBody>
where
    ReqTup<ReqBody, ResBody>: Send,
{
    type Response = CacheGetResponse<ReqBody, ConstLruProviderCacheKey>;

    type Error = ConstLruProviderError<ResBody::Error>;

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
