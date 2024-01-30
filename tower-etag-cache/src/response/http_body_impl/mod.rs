//! Implementation of `http_body::Body` for `EtagCacheResBody`
//! for `ResBody` and `TResBody` types that yield `bytes::Bytes` data.
//!
//! This allows the middleware to be easily used with axum 0.6.

use std::{
    pin::Pin,
    task::{Context, Poll},
};

use bytes::Bytes;
use http_body::{Body, Frame};

use crate::{EtagCacheResBody, EtagCacheResBodyProj};

mod err;
pub use err::*;

impl<ResBody: Body<Data = Bytes>, TResBody: Body<Data = Bytes>> Body
    for EtagCacheResBody<ResBody, TResBody>
{
    /// Data has to be Bytes due to axum's blanket IntoResponse impl
    /// for Response<B: Body<Data = Bytes>>
    type Data = Bytes;

    type Error = EtagCacheResBodyError<ResBody::Error, TResBody::Error>;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        match self.project() {
            EtagCacheResBodyProj::Miss(b) => b
                .poll_frame(cx)
                .map(|p| p.map(|res| res.map_err(EtagCacheResBodyError::Miss))),
            EtagCacheResBodyProj::Passthrough(b) => b
                .poll_frame(cx)
                .map(|p| p.map(|res| res.map_err(EtagCacheResBodyError::Passthrough))),
            EtagCacheResBodyProj::Hit => Poll::Ready(None),
        }
    }
}
