use std::{pin::Pin, task::Poll};

use http::HeaderMap;
use http_body::Body;
use pin_project::pin_project;

/// 304 responses should return empty http body
#[pin_project(project = EtagCacheRespBodyProj)]
pub enum EtagCacheRespBody<B> {
    Miss(#[pin] B),
    Hit,
}

impl<B: Body> Body for EtagCacheRespBody<B> {
    type Data = B::Data;

    type Error = B::Error;

    fn poll_data(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Result<Self::Data, Self::Error>>> {
        match self.project() {
            EtagCacheRespBodyProj::Miss(b) => b.poll_data(cx),
            EtagCacheRespBodyProj::Hit => Poll::Ready(None),
        }
    }

    fn poll_trailers(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<Option<http::HeaderMap>, Self::Error>> {
        match self.project() {
            EtagCacheRespBodyProj::Miss(b) => b.poll_trailers(cx),
            EtagCacheRespBodyProj::Hit => Poll::Ready(Ok(None)),
        }
    }
}

impl<B> EtagCacheRespBody<B> {
    pub fn hit_resp(headers: HeaderMap) -> http::Result<http::Response<Self>> {
        let mut builder = http::response::Builder::new().status(http::StatusCode::NOT_MODIFIED);
        *builder.headers_mut().unwrap() = headers;
        builder.body(Self::Hit)
    }

    pub fn miss_resp(resp: http::Response<B>) -> http::Response<Self> {
        let (parts, body) = resp.into_parts();
        http::Response::from_parts(parts, Self::Miss(body))
    }
}
