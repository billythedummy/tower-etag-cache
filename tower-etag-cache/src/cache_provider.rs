use http::HeaderMap;
use tower_service::Service;

#[derive(Debug, Clone)]
pub struct CacheGetResponse<Req, Key> {
    pub req: Req,
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

pub trait CacheGetProvider<Req>: Service<Req, Response = CacheGetResponse<Req, Self::Key>> {
    type Key;
}

/// The service passes (key, response from inner service) to the provider.
/// The provider determines if the ETag should be cached. If so, it
/// calculates the ETag value from response, stores it, modifies
/// response by adding the ETag header, and returns the modified response.
/// Else it returns the original response unmodified
pub trait CachePutProvider<Resp>: Service<(Self::Key, Resp), Response = Resp> {
    type Key;
}

pub trait CacheProvider<Req, Resp>:
    CacheGetProvider<Req, Key = Self::K> + CachePutProvider<Resp, Key = Self::K>
{
    type K;
}
