pub trait DeriveEtagCacheKey {
    type Key;
    type Error;

    /// Derive the ETag cache key.
    /// The key should be unique given the same URI and HeaderValues of the headers
    /// listed in the `Vary` header
    fn derive_etag_cache_key<T>(req: &http::Request<T>) -> Result<Self::Key, Self::Error>;
}
