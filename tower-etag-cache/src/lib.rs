#![doc = include_str!("../README.md")]

use std::task::Poll;
use tower_layer::Layer;
use tower_service::Service;

mod cache_provider;
mod err;
mod future;
mod passthrough_predicate;
mod response;

#[cfg(feature = "simple-etag-cache-key")]
pub mod simple_etag_cache_key;

#[cfg(feature = "base64-blake3-body-etag")]
pub mod base64_blake3_body_etag;

#[cfg(feature = "const-lru-provider")]
pub mod const_lru_provider;

pub use cache_provider::*;
pub use err::*;
pub use future::*;
pub use passthrough_predicate::*;
pub use response::*;

#[derive(Clone, Copy, Debug)]
pub struct EtagCache<C, P, S> {
    cache_provider: C,
    passthrough_predicate: P,
    inner: S,
}

impl<C, P, S> EtagCache<C, P, S> {
    pub fn new(cache_provider: C, passthrough_predicate: P, inner: S) -> Self {
        Self {
            cache_provider,
            passthrough_predicate,
            inner,
        }
    }
}

impl<C, S> EtagCache<C, DefaultPredicate, S> {
    pub fn with_default_predicate(cache_provider: C, inner: S) -> Self {
        Self {
            cache_provider,
            passthrough_predicate: DefaultPredicate,
            inner,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct EtagCacheLayer<C, P> {
    cache_provider: C,
    passthrough_predicate: P,
}

impl<C, P> EtagCacheLayer<C, P> {
    pub fn new(cache_provider: C, passthrough_predicate: P) -> Self {
        Self {
            cache_provider,
            passthrough_predicate,
        }
    }
}

impl<C> EtagCacheLayer<C, DefaultPredicate> {
    pub fn with_default_predicate(cache_provider: C) -> Self {
        Self {
            cache_provider,
            passthrough_predicate: DefaultPredicate,
        }
    }
}

impl<C: Clone, P: Clone, S> Layer<S> for EtagCacheLayer<C, P> {
    type Service = EtagCache<C, P, S>;

    fn layer(&self, inner: S) -> Self::Service {
        EtagCache::new(
            self.cache_provider.clone(),
            self.passthrough_predicate.clone(),
            inner,
        )
    }
}

impl<ReqBody, ResBody, C, P, S> Service<http::Request<ReqBody>> for EtagCache<C, P, S>
where
    C: CacheProvider<ReqBody, ResBody> + Clone,
    P: PassthroughPredicate,
    S: Service<http::Request<ReqBody>, Response = http::Response<ResBody>> + Clone,
{
    type Response = http::Response<EtagCacheResBody<ResBody, C::TResBody>>;

    type Error = EtagCacheServiceError<
        <C as Service<http::Request<ReqBody>>>::Error,
        S::Error,
        <C as Service<(C::Key, http::Response<ResBody>)>>::Error,
    >;

    type Future = EtagCacheServiceFuture<ReqBody, ResBody, C, P, S>;

    /// `EtagCacheServiceFuture` poll_ready()s the different services depending on whether
    /// the cache should be used
    fn poll_ready(&mut self, _cx: &mut std::task::Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: http::Request<ReqBody>) -> Self::Future {
        match self.passthrough_predicate.should_passthrough_req(&req) {
            true => EtagCacheServiceFuture::passthrough(
                self.cache_provider.clone(),
                self.passthrough_predicate.clone(),
                self.inner.clone(),
                req,
            ),
            false => EtagCacheServiceFuture::start(
                self.cache_provider.clone(),
                self.passthrough_predicate.clone(),
                self.inner.clone(),
                req,
            ),
        }
    }
}
