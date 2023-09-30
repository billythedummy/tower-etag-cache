use std::task::Poll;
use tower_service::Service;

mod cache_provider;
mod err;
mod future;
mod response;

#[cfg(feature = "simple-cache-key")]
pub mod simple_cache_key;

#[cfg(feature = "base64-blake3-body-etag")]
pub mod base64_blake3_body_etag;

pub use cache_provider::*;
pub use err::*;
pub use future::*;
pub use response::*;

#[derive(Clone, Copy, Debug)]
pub struct EtagCache<C, S> {
    cache_provider: C,
    inner: S,
}

impl<ReqBody, ResBody, C, S> Service<http::Request<ReqBody>> for EtagCache<C, S>
where
    C: CacheProvider<http::Request<ReqBody>, http::Response<ResBody>> + Clone,
    S: Service<http::Request<ReqBody>, Response = http::Response<ResBody>> + Clone,
{
    type Response = http::Response<EtagCacheRespBody<ResBody>>;

    type Error = EtagCacheServiceError<
        <C as Service<http::Request<ReqBody>>>::Error,
        S::Error,
        <C as Service<(
            <C as CachePutProvider<http::Response<ResBody>>>::Key,
            http::Response<ResBody>,
        )>>::Error,
    >;

    type Future = EtagCacheServiceFuture<ReqBody, ResBody, C, S>;

    /// `EtagCacheServiceFuture` poll_ready()s the different services depending on whether
    /// the cache should be used
    fn poll_ready(&mut self, _cx: &mut std::task::Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    /// TODO: conditional to allow requests to not interact with cache layer at all
    /// and go straight to inner
    fn call(&mut self, req: http::Request<ReqBody>) -> Self::Future {
        EtagCacheServiceFuture::cache_get_before(
            self.cache_provider.clone(),
            self.inner.clone(),
            req,
        )
    }
}
