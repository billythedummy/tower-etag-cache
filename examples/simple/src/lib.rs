use axum::{
    error_handling::HandleErrorLayer, http::StatusCode, response::Html, routing::get, BoxError,
    Form, Router,
};
use lazy_static::lazy_static;
use minijinja::{path_loader, Environment};
use minijinja_autoreload::AutoReloader;
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tower::ServiceBuilder;
use tower_etag_cache::{const_lru_provider::ConstLruProvider, EtagCacheLayer};
use tower_http::{
    compression::CompressionLayer,
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

    let app = Router::new()
        .route("/", get(home))
        .route("/index/name", get(name))
        .fallback_service(
            ServeDir::new("app") // if no files found, check your pwd to make sure it's at project root
                .fallback(ServeFile::new("app/404.html")),
        )
        .layer(CompressionLayer::new())
        .layer(
            ServiceBuilder::new()
                .layer(HandleErrorLayer::new(handle_etag_cache_layer_err))
                .layer(EtagCacheLayer::with_default_predicate(ConstLruProvider::<
                    _,
                    _,
                    255,
                    u8,
                >::init(
                    5
                ))),
        )
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        );

    let listener = TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app.into_make_service())
        .await
        .unwrap();
}

pub async fn handle_etag_cache_layer_err<T: Into<BoxError>>(err: T) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, err.into().to_string())
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
