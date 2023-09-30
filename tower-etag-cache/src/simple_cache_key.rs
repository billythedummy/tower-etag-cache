use http::header::ToStrError;

/// This calculates a string cache key using `req.uri().to_string()`
///
/// If `Vary` header is present, it appends a space (0x20, invalid URI character)
/// to the resulting string, followed by each HeaderValue of each Header specified in `Vary`.
///
/// This means `Vary: Accept-Language, Accept-Encoding` results in a different key from
/// `Vary: Accept-Encoding, Accept-Language`.
///
/// Similarly, `Accept-Language: en, fr` results in a different key from `Accept-Language: fr, en`,
/// assuming `Accept-Language` is a `Vary` value.
///
/// `Cache-Control: private` is ignored.
///
/// TODO: handle `Cache-Control: private` by looking at cookies or Auth header maybe
pub fn simple_etag_cache_key<T>(req: &http::Request<T>) -> Result<String, ToStrError> {
    let mut res = req.uri().to_string();
    let vary = req.headers().get_all("vary");
    let mut vary_iter = vary.iter().peekable();
    if vary_iter.peek().is_none() {
        return Ok(res);
    }

    res.push(' ');
    for header_name in vary_iter {
        let header_vals = req.headers().get_all(header_name.to_str()?);
        for val in header_vals {
            res.push_str(val.to_str()?);
        }
    }
    Ok(res)
}
