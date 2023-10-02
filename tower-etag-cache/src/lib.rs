use http::Method;
use std::task::Poll;
use tower_layer::Layer;
use tower_service::Service;

mod cache_provider;
mod err;
mod future;
mod response;

#[cfg(feature = "simple-cache-key")]
pub mod simple_cache_key;

#[cfg(feature = "base64-blake3-body-etag")]
pub mod base64_blake3_body_etag;

#[cfg(feature = "const-lru-provider")]
pub mod const_lru_provider;

pub use cache_provider::*;
pub use err::*;
pub use future::*;
pub use response::*;

#[derive(Clone, Copy, Debug)]
pub struct EtagCache<C, S> {
    cache_provider: C,
    inner: S,
}

impl<C, S> EtagCache<C, S> {
    pub fn new(cache_provider: C, inner: S) -> Self {
        Self {
            cache_provider,
            inner,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct EtagCacheLayer<C> {
    cache_provider: C,
}

impl<C> EtagCacheLayer<C> {
    pub fn new(cache_provider: C) -> Self {
        Self { cache_provider }
    }
}

impl<C: Clone, S> Layer<S> for EtagCacheLayer<C> {
    type Service = EtagCache<C, S>;

    fn layer(&self, inner: S) -> Self::Service {
        EtagCache::new(self.cache_provider.clone(), inner)
    }
}

impl<ReqBody, ResBody, C, S> Service<http::Request<ReqBody>> for EtagCache<C, S>
where
    C: CacheProvider<ReqBody, ResBody> + Clone,
    S: Service<http::Request<ReqBody>, Response = http::Response<ResBody>> + Clone,
{
    type Response = http::Response<EtagCacheResBody<ResBody, C::TResBody>>;

    type Error = EtagCacheServiceError<
        <C as Service<http::Request<ReqBody>>>::Error,
        S::Error,
        <C as Service<(C::Key, http::Response<ResBody>)>>::Error,
    >;

    type Future = EtagCacheServiceFuture<ReqBody, ResBody, C, S>;

    /// `EtagCacheServiceFuture` poll_ready()s the different services depending on whether
    /// the cache should be used
    fn poll_ready(&mut self, _cx: &mut std::task::Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    /// TODO: additional user-specified conditionals to control request passthrough
    fn call(&mut self, req: http::Request<ReqBody>) -> Self::Future {
        match *req.method() {
            Method::GET | Method::HEAD => {
                EtagCacheServiceFuture::start(self.cache_provider.clone(), self.inner.clone(), req)
            }
            _ => EtagCacheServiceFuture::passthrough(
                self.cache_provider.clone(),
                self.inner.clone(),
                req,
            ),
        }
    }
}
