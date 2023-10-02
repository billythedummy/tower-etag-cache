use axum::{
    body::{Body, BoxBody},
    error_handling::HandleErrorLayer,
    http::StatusCode,
    response::Html,
    routing::get,
    BoxError, Form, Router,
};
use lazy_static::lazy_static;
use minijinja::{path_loader, Environment};
use minijinja_autoreload::AutoReloader;
use serde::{Deserialize, Serialize};
use tower::ServiceBuilder;
use tower_etag_cache::{const_lru_provider::ConstLruProvider, EtagCacheLayer};
use tower_http::{
    compression::Compression,
    services::{ServeDir, ServeFile},
    trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer},
};
use tracing::Level;

pub const TEMPLATE_PATH: &str = "app/templates";

lazy_static! {
    pub static ref TEMPLATES: AutoReloader = {
        AutoReloader::new(|notifier| {
            let mut env = Environment::new();
            env.set_loader(path_loader(TEMPLATE_PATH));
            notifier.watch_path(TEMPLATE_PATH, true);
            Ok(env)
        })
    };
}

pub async fn main() {
    tracing_subscriber::fmt()
        .with_target(false)
        .compact()
        .init();

    let static_files = Compression::new(
        ServeDir::new("app") // if no files found, check your pwd to make sure it's at project root
            .fallback(ServeFile::new("app/404.html")),
    );

    let const_lru_provider_handle = ConstLruProvider::<Body, BoxBody, 255, u8>::init(1);

    let app = Router::new()
        .route("/", get(home))
        .route("/index/name", get(name))
        .layer(
            ServiceBuilder::new()
                .layer(HandleErrorLayer::new(handle_etag_cache_layer_err))
                .layer(EtagCacheLayer::new(const_lru_provider_handle)),
        )
        .fallback_service(static_files)
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        );

    axum::Server::bind(&"0.0.0.0:3000".parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();
}

/// TODO
pub async fn handle_etag_cache_layer_err<T: Into<BoxError>>(_err: T) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, "".into())
}

pub async fn home() -> axum::response::Result<Html<String>> {
    let env = TEMPLATES.acquire_env().unwrap();
    let template = env.get_template("index.html").unwrap();
    let html = template
        .render(())
        .map_err(|_| StatusCode::from_u16(500).unwrap())?;
    Ok(html.into())
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Name {
    pub name: String,
}

pub async fn name(Form(mut name): Form<Name>) -> axum::response::Result<Html<String>> {
    name.name = ammonia::clean(&name.name);
    let env = TEMPLATES.acquire_env().unwrap();
    let template = env.get_template("name.html").unwrap();
    let html = template
        .render(&name)
        .map_err(|_| StatusCode::from_u16(500).unwrap())?;
    Ok(html.into())
}
