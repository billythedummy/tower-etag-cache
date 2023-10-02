use std::{
    error::Error,
    fmt::{Debug, Display},
};

#[derive(Debug)]
pub enum EtagCacheServiceError<CacheGetError, InnerError, CachePutError> {
    CacheGetError(CacheGetError),
    InnerError(InnerError),
    CachePutError(CachePutError),
    ResponseError(http::Error),
}

impl<CacheGetError: Display, InnerError: Display, CachePutError: Display> Display
    for EtagCacheServiceError<CacheGetError, InnerError, CachePutError>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CacheGetError(e) => e.fmt(f),
            Self::InnerError(e) => e.fmt(f),
            Self::CachePutError(e) => e.fmt(f),
            Self::ResponseError(e) => std::fmt::Display::fmt(&e, f),
        }
    }
}

impl<
        CacheGetError: Display + Debug,
        InnerError: Display + Debug,
        CachePutError: Display + Debug,
    > Error for EtagCacheServiceError<CacheGetError, InnerError, CachePutError>
{
}
