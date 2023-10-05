# tower-etag-cache

A [tower](https://github.com/tower-rs) middleware for implementing [ETag-based HTTP caching](https://developer.mozilla.org/en-US/docs/Web/HTTP/Caching#etagif-none-match).

## Quickstart

The `const-lru-provider` feature provides a singleton [const-lru](https://docs.rs/const-lru/latest/const_lru)-backed [`CacheProvider`](CacheProvider) implementation that's ready to be used.

```rust ignore
use axum::{error_handling::HandleErrorLayer, http::StatusCode, BoxError, Router};
use tower_etag_cache::{const_lru_provider::ConstLruProvider, EtagCacheLayer};
use tower_http::services::{ServeDir, ServeFile};

#[tokio::main]
pub async fn main() {
    let app = Router::new()
        .fallback_service(ServeDir::new("app").fallback(ServeFile::new("app/404.html")))
        .layer(
            ServiceBuilder::new()
                .layer(HandleErrorLayer::new(handle_etag_cache_layer_err))
                .layer(EtagCacheLayer::with_default_predicate(
                    ConstLruProvider::<_, _, 255, u8>::init(5),
                )),
        );
    axum::Server::bind(&"0.0.0.0:3000".parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn handle_etag_cache_layer_err<T: Into<BoxError>>(err: T) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, err.into().to_string())
}
```

The [`ConstLruProvider`](const_lru_provider::ConstLruProvider) calculates ETag as the base64-encoded blake3 hash of response bodies.

It keys entries by [`SimpleEtagCacheKey`](simple_etag_cache_key::SimpleEtagCacheKey), a struct comprising the request URI + sorted `Vec` collections of header values for the `Accept`, `Accept-Language`, and `Accept-Encoding` request headers. This causes it to [vary](https://developer.mozilla.org/en-US/docs/Web/HTTP/Caching#vary) ETags based on these headers.

Since the current implementation loads the entire response body into memory to calculate the ETag, [`ConstLruProvider`](const_lru_provider::ConstLruProvider) is not suitable for extremely large responses such as large files.

## How This Works

The [`EtagCache`](EtagCache) tower service and [`EtagCacheLayer`](EtagCacheLayer) tower layer is created with an inner tower service + any type that implements the [`CacheProvider`](CacheProvider) trait. 

A [`CacheProvider`](CacheProvider):
- comprises 2 tower services
    - 1 that runs on incoming http requests to lookup ETags to check if a request's `If-None-Match` matches an ETag in the cache
    - 1 that runs on outgoing http responses to calculate and save the ETag of the response
- has an associated cache key type that is used to key cache entries
- has an associated transform response body type that it transforms outgoing http response bodies into after the ETag calculation and saving procedure 

```rust ignore
pub trait CacheProvider<ReqBody, ResBody>:
    Service<http::Request<ReqBody>, Response = CacheGetResponse<ReqBody, Self::Key>> // runs on request
    + Service<(Self::Key, http::Response<ResBody>), Response = http::Response<Self::TResBody>> // runs on response
{
    type Key;
    type TResBody;
}
```

When a http request comes in,
- If the service's passthrough_predicate indicates that the request should be passed through, the unmodified request is passed directly to the inner service.
- Else the [`CacheProvider`](CacheProvider)'s first ETag lookup service runs on the request.
- If the service returns a cache hit, an empty HTTP 304 response is returned to the client with the relevant headers.
- Else the inner service runs on the unmodified request.
- If the service's passthrough_predicate indicates that the response should be passed through, the unmodified response is returned to the client.
- Else the [`CacheProvider`](CacheProvider)'s second ETag calculating and saving service runs on the http response returned by the inner service.
- The service transforms the response body and modifies the response headers to include the saved ETag and other relevant headers and returns it to the client.

### PassthroughPredicate

The [`PassthroughPredicate`](PassthroughPredicate) trait controls when requests and responses should ignore the caching layer.

The provided [`DefaultPredicate`](DefaultPredicate) is available for use with [`EtagCacheLayer::with_default_predicate`](EtagCacheLayer::with_default_predicate) and has the following behaviour:

requests:
- only `GET` and `HEAD` methods are ran through the caching layer

responses:
- only `HTTP 2XX` responses, excluding `204 No Content`, are cached
- only responses that dont already have the `ETag` header are cached
- only responses that eiter have a missing, invalid, or non-zero `Content-Length` header are cached 
