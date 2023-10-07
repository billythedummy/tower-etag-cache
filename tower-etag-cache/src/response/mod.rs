use http::HeaderMap;
use pin_project::pin_project;

#[cfg(feature = "http-body-impl")]
pub mod http_body_impl;

/// `http::Response` body type of [`EtagCache`](crate::EtagCache)
#[pin_project(project = EtagCacheResBodyProj)]
pub enum EtagCacheResBody<ResBody, TResBody> {
    Miss(#[pin] TResBody),
    Passthrough(#[pin] ResBody),

    /// 304 response. Should return empty http body
    Hit,
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
