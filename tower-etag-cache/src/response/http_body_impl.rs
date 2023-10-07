//! Implementation of `http_body::Body` for `EtagCacheResBody`
//! for `ResBody` and `TResBody` types that yield `bytes::Bytes` data.
//!
//! This allows the middleware to be easily used with axum 0.6.

use std::{pin::Pin, task::Poll};

use bytes::Bytes;
use http_body::Body;

use crate::{EtagCacheResBody, EtagCacheResBodyError, EtagCacheResBodyProj};

impl<ResBody: Body<Data = Bytes>, TResBody: Body<Data = Bytes>> Body
    for EtagCacheResBody<ResBody, TResBody>
{
    /// Data has to be Bytes due to axum's blanket IntoResponse impl
    /// for Response<B: Body<Data = Bytes>>
    type Data = Bytes;

    type Error = EtagCacheResBodyError<ResBody::Error, TResBody::Error>;

    fn poll_data(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Result<Self::Data, Self::Error>>> {
        match self.project() {
            EtagCacheResBodyProj::Miss(b) => b
                .poll_data(cx)
                .map(|p| p.map(|res| res.map_err(EtagCacheResBodyError::Miss))),
            EtagCacheResBodyProj::Passthrough(b) => b
                .poll_data(cx)
                .map(|p| p.map(|res| res.map_err(EtagCacheResBodyError::Passthrough))),
            EtagCacheResBodyProj::Hit => Poll::Ready(None),
        }
    }

    fn poll_trailers(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<Option<http::HeaderMap>, Self::Error>> {
        match self.project() {
            EtagCacheResBodyProj::Miss(b) => b
                .poll_trailers(cx)
                .map(|res| res.map_err(EtagCacheResBodyError::Miss)),
            EtagCacheResBodyProj::Passthrough(b) => b
                .poll_trailers(cx)
                .map(|res| res.map_err(EtagCacheResBodyError::Passthrough)),
            EtagCacheResBodyProj::Hit => Poll::Ready(Ok(None)),
        }
    }
}
