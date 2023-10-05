use http::{
    header::{ACCEPT, ACCEPT_ENCODING, ACCEPT_LANGUAGE, VARY},
    HeaderMap, HeaderName, HeaderValue,
};

/// Cache key derived from uri, and varying by the following request headers:
/// - Accept
/// - Accept-Encoding
/// - Accept-Language
///
/// Handles multiple header values for the same header name by storing them in a sorted Vec
///
/// `Cache-control: private` is ignored
#[derive(Debug, Default, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SimpleEtagCacheKey {
    pub uri_string: String,
    pub accept: Vec<HeaderValue>,
    pub accept_encoding: Vec<HeaderValue>,
    pub accept_language: Vec<HeaderValue>,
}

impl SimpleEtagCacheKey {
    /// Sets the required `Vary` response headers for a response
    pub fn set_response_headers(headers_mut: &mut HeaderMap) {
        headers_mut.append(VARY, HeaderValue::from_static(ACCEPT.as_str()));
        headers_mut.append(VARY, HeaderValue::from_static(ACCEPT_ENCODING.as_str()));
        headers_mut.append(VARY, HeaderValue::from_static(ACCEPT_LANGUAGE.as_str()));
    }
}

pub fn calc_simple_etag_cache_key<T>(req: &http::Request<T>) -> SimpleEtagCacheKey {
    let headers = req.headers();
    SimpleEtagCacheKey {
        uri_string: req.uri().to_string(),
        accept: calc_header_key_component(headers, ACCEPT),
        accept_encoding: calc_header_key_component(headers, ACCEPT_ENCODING),
        accept_language: calc_header_key_component(headers, ACCEPT_LANGUAGE),
    }
}

fn calc_header_key_component(headers: &HeaderMap, name: HeaderName) -> Vec<HeaderValue> {
    let mut res: Vec<_> = headers.get_all(name).iter().cloned().collect();
    res.sort_unstable();
    res
}
