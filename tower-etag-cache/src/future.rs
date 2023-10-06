use pin_project::pin_project;
use std::{
    future::Future,
    mem::ManuallyDrop,
    pin::Pin,
    task::{Context, Poll},
};
use tower_service::Service;

use crate::{
    cache_provider::CacheProvider, CacheGetResponse, CacheGetResponseResult, EtagCacheResBody,
    EtagCacheServiceError, PassthroughPredicate,
};

#[pin_project]
pub struct EtagCacheServiceFuture<
    ReqBody,
    ResBody,
    C: CacheProvider<ReqBody, ResBody>,
    P: PassthroughPredicate,
    S: Service<http::Request<ReqBody>, Response = http::Response<ResBody>>,
> {
    cache_provider: C,
    passthrough_predicate: P,
    inner: S,
    #[pin]
    state: EtagCacheServiceFutureState<ReqBody, ResBody, C, S>,
}

impl<
        ReqBody,
        ResBody,
        C: CacheProvider<ReqBody, ResBody>,
        P: PassthroughPredicate,
        S: Service<http::Request<ReqBody>, Response = http::Response<ResBody>>,
    > EtagCacheServiceFuture<ReqBody, ResBody, C, P, S>
{
    pub fn start(
        cache_provider: C,
        passthrough_predicate: P,
        inner: S,
        req: http::Request<ReqBody>,
    ) -> Self {
        Self {
            cache_provider,
            passthrough_predicate,
            inner,
            state: EtagCacheServiceFutureState::CacheGetBefore {
                req: ManuallyDrop::new(req),
            },
        }
    }

    pub fn passthrough(
        cache_provider: C,
        passthrough_predicate: P,
        inner: S,
        req: http::Request<ReqBody>,
    ) -> Self {
        Self {
            cache_provider,
            passthrough_predicate,
            inner,
            state: EtagCacheServiceFutureState::InnerBefore {
                key: None,
                req: ManuallyDrop::new(req),
            },
        }
    }
}

// Use ManuallyDrop to allow easy moving of fields behind Pin<&mut self> to the next state
#[pin_project(project = EtagCacheServiceFutureStateProj)]
pub enum EtagCacheServiceFutureState<
    ReqBody,
    ResBody,
    C: CacheProvider<ReqBody, ResBody>,
    S: Service<http::Request<ReqBody>, Response = http::Response<ResBody>>,
> {
    CacheGetBefore {
        req: ManuallyDrop<http::Request<ReqBody>>,
    },
    CacheGet {
        #[pin]
        fut: <C as Service<http::Request<ReqBody>>>::Future,
    },
    InnerBefore {
        /// None indicates req passthrough: only inner service is called
        key: Option<C::Key>,
        req: ManuallyDrop<http::Request<ReqBody>>,
    },
    Inner {
        /// None indicates req passthrough: only inner service is called
        key: Option<C::Key>,
        #[pin]
        fut: S::Future,
    },
    CachePutBefore {
        key: ManuallyDrop<C::Key>,
        resp: ManuallyDrop<http::Response<ResBody>>,
    },
    CachePut {
        #[pin]
        fut: <C as Service<(C::Key, http::Response<ResBody>)>>::Future,
    },
}

impl<
        ReqBody,
        ResBody,
        C: CacheProvider<ReqBody, ResBody>,
        P: PassthroughPredicate,
        S: Service<http::Request<ReqBody>, Response = http::Response<ResBody>>,
    > Future for EtagCacheServiceFuture<ReqBody, ResBody, C, P, S>
{
    type Output = Result<
        http::Response<EtagCacheResBody<ResBody, C::TResBody>>,
        EtagCacheServiceError<
            <C as Service<http::Request<ReqBody>>>::Error,
            S::Error,
            <C as Service<(C::Key, http::Response<ResBody>)>>::Error,
        >,
    >;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let mut curr_state = this.state;

        match curr_state.as_mut().project() {
            EtagCacheServiceFutureStateProj::CacheGetBefore { req } => {
                match <C as Service<http::Request<ReqBody>>>::poll_ready(this.cache_provider, cx) {
                    Poll::Pending => Poll::Pending,
                    Poll::Ready(result) => {
                        if let Err(e) = result {
                            return Poll::Ready(Err(EtagCacheServiceError::CacheGetError(e)));
                        }
                        let fut = <C as Service<http::Request<ReqBody>>>::call(
                            this.cache_provider,
                            unsafe { ManuallyDrop::take(req) },
                        );
                        curr_state.set(EtagCacheServiceFutureState::CacheGet { fut });
                        cx.waker().wake_by_ref();
                        Poll::Pending
                    }
                }
            }
            EtagCacheServiceFutureStateProj::CacheGet { fut } => match fut.poll(cx) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(result) => {
                    let CacheGetResponse { req, result } = match result {
                        Ok(r) => r,
                        Err(e) => return Poll::Ready(Err(EtagCacheServiceError::CacheGetError(e))),
                    };
                    let key = match result {
                        CacheGetResponseResult::Hit(headers) => {
                            return Poll::Ready(
                                EtagCacheResBody::hit_resp(headers)
                                    .map_err(EtagCacheServiceError::ResponseError),
                            );
                        }
                        CacheGetResponseResult::Miss(k) => k,
                    };
                    curr_state.set(EtagCacheServiceFutureState::InnerBefore {
                        key: Some(key),
                        req: ManuallyDrop::new(req),
                    });
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
            },
            EtagCacheServiceFutureStateProj::InnerBefore { key, req } => {
                match this.inner.poll_ready(cx) {
                    Poll::Pending => Poll::Pending,
                    Poll::Ready(result) => {
                        if let Err(e) = result {
                            return Poll::Ready(Err(EtagCacheServiceError::InnerError(e)));
                        }
                        let k = key.take();
                        let fut = this.inner.call(unsafe { ManuallyDrop::take(req) });
                        curr_state.set(EtagCacheServiceFutureState::Inner { fut, key: k });
                        cx.waker().wake_by_ref();
                        Poll::Pending
                    }
                }
            }
            EtagCacheServiceFutureStateProj::Inner { key, fut } => match fut.poll(cx) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(result) => {
                    let resp = match result {
                        Ok(r) => r,
                        Err(e) => return Poll::Ready(Err(EtagCacheServiceError::InnerError(e))),
                    };

                    if this.passthrough_predicate.should_passthrough_resp(&resp) {
                        return Poll::Ready(Ok(EtagCacheResBody::passthrough_resp(resp)));
                    }

                    let k = match key.take() {
                        Some(k) => k,
                        None => return Poll::Ready(Ok(EtagCacheResBody::passthrough_resp(resp))),
                    };
                    curr_state.set(EtagCacheServiceFutureState::CachePutBefore {
                        key: ManuallyDrop::new(k),
                        resp: ManuallyDrop::new(resp),
                    });
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
            },
            EtagCacheServiceFutureStateProj::CachePutBefore { key, resp } => {
                match <C as Service<(C::Key, http::Response<ResBody>)>>::poll_ready(
                    this.cache_provider,
                    cx,
                ) {
                    Poll::Pending => Poll::Pending,
                    Poll::Ready(result) => {
                        if let Err(e) = result {
                            return Poll::Ready(Err(EtagCacheServiceError::CachePutError(e)));
                        }
                        let fut = <C as Service<(C::Key, http::Response<ResBody>)>>::call(
                            this.cache_provider,
                            (unsafe { ManuallyDrop::take(key) }, unsafe {
                                ManuallyDrop::take(resp)
                            }),
                        );
                        curr_state.set(EtagCacheServiceFutureState::CachePut { fut });
                        cx.waker().wake_by_ref();
                        Poll::Pending
                    }
                }
            }
            EtagCacheServiceFutureStateProj::CachePut { fut } => match fut.poll(cx) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(result) => Poll::Ready(
                    result
                        .map(EtagCacheResBody::miss_resp)
                        .map_err(EtagCacheServiceError::CachePutError),
                ),
            },
        }
    }
}
