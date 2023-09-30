use tower_service::Service;

pub struct CacheGetResponse<Req, Key, Time> {
    pub req: Req,
    pub result: CacheGetResponseResult<Key, Time>,
}

/// Either
/// - calculated cache key if entry not in cache, so that the request
///   key can be processed by the inner service and the key can be used to put later on
/// - The time at which the entry was cached if in cache. This also serves as the HTTP Last-Modified value
pub enum CacheGetResponseResult<Key, Time> {
    Miss(Key),
    // TODO: refactor to return HeaderMap
    Hit(Time),
}

pub trait CacheGetProvider<Req>:
    Service<Req, Response = CacheGetResponse<Req, Self::Key, Self::Time>>
{
    type Key;
    type Time;
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
    CacheGetProvider<Req, Key = Self::K, Time = Self::T> + CachePutProvider<Resp, Key = Self::K>
{
    type K;
    type T;
}
