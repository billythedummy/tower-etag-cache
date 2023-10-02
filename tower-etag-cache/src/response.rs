use std::{error::Error, fmt::Display, pin::Pin, task::Poll};

use bytes::{Buf, Bytes};
use http::HeaderMap;
use http_body::Body;
use pin_project::pin_project;

/// 304 responses should return empty http body
#[pin_project(project = EtagCacheResBodyProj)]
pub enum EtagCacheResBody<ResBody, TResBody> {
    Miss(#[pin] TResBody),
    Passthrough(#[pin] ResBody),
    Hit,
}

pub enum EtagCacheResBodyData<ResBodyData, TResBodyData> {
    Miss(TResBodyData),
    Passthrough(ResBodyData),
}

impl<ResBodyData: Buf, TResBodyData: Buf> Buf for EtagCacheResBodyData<ResBodyData, TResBodyData> {
    fn remaining(&self) -> usize {
        match self {
            Self::Miss(b) => b.remaining(),
            Self::Passthrough(b) => b.remaining(),
        }
    }

    fn chunk(&self) -> &[u8] {
        match self {
            Self::Miss(b) => b.chunk(),
            Self::Passthrough(b) => b.chunk(),
        }
    }

    fn advance(&mut self, cnt: usize) {
        match self {
            Self::Miss(b) => b.advance(cnt),
            Self::Passthrough(b) => b.advance(cnt),
        }
    }
}

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

impl<ResBodyError: std::fmt::Debug + Display, TResBodyError: std::fmt::Debug + Display> Error
    for EtagCacheResBodyError<ResBodyError, TResBodyError>
{
}

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

impl<ResBody, TResBody> EtagCacheResBody<ResBody, TResBody> {
    pub fn hit_resp(headers: HeaderMap) -> http::Result<http::Response<Self>> {
        let mut builder = http::response::Builder::new().status(http::StatusCode::NOT_MODIFIED);
        *builder.headers_mut().unwrap() = headers;
        builder.body(Self::Hit)
    }

    pub fn passthrough_resp(resp: http::Response<ResBody>) -> http::Response<Self> {
        let (parts, body) = resp.into_parts();
        http::Response::from_parts(parts, Self::Passthrough(body))
    }

    pub fn miss_resp(resp: http::Response<TResBody>) -> http::Response<Self> {
        let (parts, body) = resp.into_parts();
        http::Response::from_parts(parts, Self::Miss(body))
    }
}
