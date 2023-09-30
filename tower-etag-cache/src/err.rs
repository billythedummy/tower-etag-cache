#[derive(Debug)]
pub enum EtagCacheServiceError<CacheGetError, InnerError, CachePutError> {
    CacheGetError(CacheGetError),
    InnerError(InnerError),
    CachePutError(CachePutError),
    ResponseError(http::Error),
}
