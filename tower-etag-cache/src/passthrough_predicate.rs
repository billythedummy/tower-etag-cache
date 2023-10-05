use http::{
    header::{CONTENT_LENGTH, ETAG},
    Method,
};

pub trait PassthroughPredicate: Clone {
    /// Returns true if the given request should ignore the 2 EtagCache services
    /// and only be processed by the inner service
    fn should_passthrough_req<T>(&mut self, req: &http::Request<T>) -> bool;

    /// Returns true if the given inner service response shouldn't have its ETag
    /// calculated and cached by the second EtagCache service
    fn should_passthrough_resp<T>(&mut self, resp: &http::Response<T>) -> bool;
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, PartialOrd, Ord, Hash)]
pub struct DefaultPredicate;

impl PassthroughPredicate for DefaultPredicate {
    /// Only run GET and HEAD methods through cache services
    fn should_passthrough_req<T>(&mut self, req: &http::Request<T>) -> bool {
        !matches!(*req.method(), Method::GET | Method::HEAD)
    }

    /// Only cache:
    /// - 2XX responses excluding 204 No Content
    /// - responses that dont already have ETag header set
    /// - responses that either dont have a valid Content-Length header or have a non-zero Content-Length
    fn should_passthrough_resp<T>(&mut self, resp: &http::Response<T>) -> bool {
        match resp.status().as_u16() {
            200..=203 | 205..=299 => (),
            _ => return true,
        }
        if resp.headers().contains_key(ETAG) {
            return true;
        }
        let content_length_hv = match resp.headers().get(CONTENT_LENGTH) {
            Some(s) => s,
            None => return false,
        };
        let content_length_str = match content_length_hv.to_str() {
            Ok(s) => s,
            Err(_) => return false,
        };
        let content_length: usize = match content_length_str.parse() {
            Ok(u) => u,
            Err(_) => return false,
        };
        content_length == 0
    }
}
