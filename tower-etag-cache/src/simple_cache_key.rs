use http::header::ToStrError;

/// This calculates a string cache key using `req.uri().to_string()`
///
/// `Cache-Control: private` is ignored.
///
/// TODO: handle `Cache-Control: private` by looking at cookies or Auth header maybe
/// TODO: handle `Vary` header in responses
pub fn simple_etag_cache_key<T>(req: &http::Request<T>) -> Result<String, ToStrError> {
    Ok(req.uri().to_string())
}
