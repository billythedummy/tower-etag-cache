use base64::{engine::general_purpose, Engine};
use http::{header::InvalidHeaderValue, HeaderValue};

/// Calculates the etag value as base64 encoded blake3 hash of the body bytes
pub fn base64_blake3_body_etag(
    body: impl Iterator<Item = u8>,
) -> Result<HeaderValue, InvalidHeaderValue> {
    let mut hasher = blake3::Hasher::new();
    // TODO: probably faster to do in chunks of 1024 because thats what update() works on
    for byte in body {
        hasher.update(&[byte]);
    }
    let bytes = hasher.finalize();
    let val = general_purpose::STANDARD.encode(bytes.as_bytes());
    HeaderValue::from_str(&val)
}
