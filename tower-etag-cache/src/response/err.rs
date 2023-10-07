use std::{
    error::Error,
    fmt::{Debug, Display},
};

#[derive(Debug, Clone, Copy)]
pub enum EtagCacheResBodyError<ResBodyError, TResBodyError> {
    Miss(TResBodyError),
    Passthrough(ResBodyError),
}

impl<ResBodyError: Display, TResBodyError: Display> Display
    for EtagCacheResBodyError<ResBodyError, TResBodyError>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Miss(e) => e.fmt(f),
            Self::Passthrough(e) => e.fmt(f),
        }
    }
}

impl<ResBodyError: Debug + Display, TResBodyError: Debug + Display> Error
    for EtagCacheResBodyError<ResBodyError, TResBodyError>
{
}
