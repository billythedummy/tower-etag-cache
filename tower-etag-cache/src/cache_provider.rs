use http::HeaderMap;
use tower_service::Service;

#[derive(Debug)]
pub struct CacheGetResponse<ReqBody, Key> {
    /// Pass on the original request so that it can be processed by the inner service
    pub req: http::Request<ReqBody>,
    pub result: CacheGetResponseResult<Key>,
}

/// Either
/// - calculated cache key if entry not in cache, so that the key can be used to put later on
/// - HTTP response headers to send along with the HTTP 304 response if entry in cache
#[derive(Debug, Clone)]
pub enum CacheGetResponseResult<Key> {
    Miss(Key),
    Hit(HeaderMap),
}

/// Typical type args for use in axum 0.6:
///
/// ```ignore
/// ReqBody = hyper::body::Body
/// ResBody = axum::body::BoxBody
/// ```
pub trait CacheProvider<ReqBody, ResBody>:
    Service<http::Request<ReqBody>, Response = CacheGetResponse<ReqBody, Self::Key>> // Get
    + Service<(Self::Key, http::Response<ResBody>), Response = http::Response<Self::TResBody>> // Put
{
    type Key;
    type TResBody;
}
