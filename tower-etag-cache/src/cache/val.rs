pub trait CalcEtag {
    type Body;
    type Error;

    /// Calculates the ETag value from the response.
    fn calc_etag(body: Self::Body) -> Result<http::HeaderValue, Self::Error>;
}
