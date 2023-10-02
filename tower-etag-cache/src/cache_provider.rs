use http::HeaderMap;
use tower_service::Service;

#[derive(Debug)]
pub struct CacheGetResponse<ReqBody, Key> {
    pub req: http::Request<ReqBody>,
    pub result: CacheGetResponseResult<Key>,
}

/// Either
/// - calculated cache key if entry not in cache, so that the request
///   key can be processed by the inner service and the key can be used to put later on
/// - HTTP response headers to send along with the HTTP 304 response
#[derive(Debug, Clone)]
pub enum CacheGetResponseResult<Key> {
    Miss(Key),
    Hit(HeaderMap),
}

pub trait CacheProvider<ReqBody, ResBody>:
    Service<http::Request<ReqBody>, Response = CacheGetResponse<ReqBody, Self::Key>> // Get
    + Service<(Self::Key, http::Response<ResBody>), Response = http::Response<Self::TResBody>> // Put
{
    type Key;
    type TResBody;
}
