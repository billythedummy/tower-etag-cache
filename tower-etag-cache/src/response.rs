use std::{pin::Pin, task::Poll};

use http::{
    header::{AsHeaderName, IntoHeaderName},
    HeaderMap,
};
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
    /// TODO redo
    pub fn hit_resp<R, T>(req: http::Request<R>, _time: T) -> http::Response<Self> {
        let (parts, _) = req.into_parts();
        let req_headers = parts.headers;

        let mut builder = http::response::Builder::new().status(http::StatusCode::NOT_MODIFIED);
        if let Some(resp_headers) = builder.headers_mut() {
            for (req_header_name, resp_header_name) in
                [(http::header::IF_NONE_MATCH, http::header::ETAG)]
            {
                trf_header_req_to_resp(
                    &req_headers,
                    resp_headers,
                    req_header_name,
                    resp_header_name,
                )
            }
        }

        builder.body(Self::Hit).unwrap()
    }

    pub fn miss_resp(resp: http::Response<B>) -> http::Response<Self> {
        let (parts, body) = resp.into_parts();
        http::Response::from_parts(parts, Self::Miss(body))
    }
}

fn trf_header_req_to_resp<ReqK: AsHeaderName, RespK: IntoHeaderName>(
    req_headers: &HeaderMap,
    resp_headers: &mut HeaderMap,
    req_header_name: ReqK,
    resp_header_name: RespK,
) {
    let req_val = match req_headers.get(req_header_name) {
        Some(v) => v,
        None => return,
    };
    resp_headers.append(resp_header_name, req_val.clone());
}
