use std::ops::Deref;

use base64::{engine::general_purpose, Engine};
use http::HeaderValue;

/// Calculates the etag value as base64 encoded blake3 hash of the body bytes
pub fn base64_blake3_body_etag<I: Deref<Target = u8>>(
    body: impl Iterator<Item = I>,
) -> HeaderValue {
    let mut hasher = blake3::Hasher::new();
    // TODO: probably faster to do in chunks of 1024 because thats what update() works on
    for byte in body {
        hasher.update(&[*byte]);
    }
    let bytes = hasher.finalize();
    let val = general_purpose::STANDARD.encode(bytes.as_bytes());
    // base64 should be always valid ascii
    HeaderValue::from_str(&val).unwrap()
}
