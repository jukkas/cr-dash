use axum::{
    Router, body::Body, handler::HandlerWithoutStateExt, http::{self, Request, Response, StatusCode}, routing::get
};

use dotenvy::dotenv;
use std::env;
use log::info;
//use sqlx::SqlitePool;

use base64::{self, Engine, prelude::BASE64_STANDARD};
use std::time::Instant;
use tower_http::services::ServeDir;
use tower_sessions_cookie_store::{CookieSessionConfig, CookieSessionManagerLayer, Key, SameSite};
use std::sync::{Arc, Mutex};

mod containers;
mod github;

// Avoid musl's default allocator due to lackluster performance
// https://nickb.dev/blog/default-musl-allocator-considered-harmful-to-performance
#[cfg(target_env = "musl")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[derive(Clone, Default)]
pub struct AppState {
    cached_catalog: Arc<Mutex<containers::CachedCatalog>>,
}

#[tokio::main]
async fn main() {
    dotenv().ok();
    env_logger::init();
    let port = env::var("APP_PORT").unwrap_or("3000".to_string());

    let s404_service = handle_404.into_service();
    let serve_dir = ServeDir::new("static").not_found_service(s404_service);

    // Session
    let session_key =
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA==";
    let session_key = std::env::var("SESSION_KEY").unwrap_or(session_key.to_string());
    println!("session_key: {session_key}");
    let session_key = BASE64_STANDARD.decode(session_key.as_bytes()).expect("Failed to decode SESSION_KEY");

    // let session_key = Key::generate();
    let session_key = Key::from(&session_key);
    let session_config = CookieSessionConfig::default().with_secure(false).with_same_site(SameSite::Lax);

    let app_state = AppState::default();

    let app = Router::new()
        .route("/health", get(empty))
        .route("/", get(containers::catalog))
        .route("/tags/{*path}", get(containers::tags))
        .route("/login", get(github::login))
        .route("/callback", get(github::callback))
        .fallback_service(serve_dir)
        .with_state(app_state)
        .layer(CookieSessionManagerLayer::private(session_key).with_config(session_config))
        .layer(
            tower::ServiceBuilder::new().layer(axum::middleware::from_fn(simple_log_middleware)),
        );

    info!("Starting server on port {}", port);
    // run it with hyper on localhost:3000
    let listener = tokio::net::TcpListener::bind(&format!("{}:{}", "[::]", port))
        .await
        .unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn handle_404() -> (StatusCode, &'static str) {
    (StatusCode::NOT_FOUND, "Not found 404 !!!!!")
}
// Empty response
async fn empty() {}

pub async fn simple_log_middleware(
    request: Request<Body>,
    next: axum::middleware::Next,
) -> Result<Response<Body>, StatusCode> {
    //use hyper::body::to_bytes;
    //use http::header::CONTENT_TYPE;

    let method = request.method().clone();
    let uri = request.uri().clone();
    let start = Instant::now();
    let from = request
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();

    // Only attempt to read/print the body for POST and JSON content-type
    let mut request = request;
    if method == http::Method::POST {
        if let Some(ct) = request.headers().get(http::header::CONTENT_TYPE) {
            if ct.to_str().map(|s| s.contains("json")).unwrap_or(false) {
                // take body, read bytes, then put a fresh body back onto the request
                let (parts, body) = request.into_parts();
                let bytes = axum::body::to_bytes(body, 1024 * 1024)
                    .await
                    .unwrap_or_default(); // 1MB limit
                // log (beware of large/sensitive payloads)
                let body_text = String::from_utf8_lossy(&bytes);
                println!("REQUEST JSON BODY: {}", body_text);
                // reconstruct request with the same parts and the consumed bytes
                request = Request::from_parts(parts, Body::from(bytes));
            }
        }
    }

    let response = next.run(request).await;

    let status = response.status();
    let duration = start.elapsed().as_millis();

        println!(
        "{} {} {} {} {}ms",
        from,
        method,
        uri,
        status.as_u16(),
        duration
    );

    Ok(response)
}
