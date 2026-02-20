use axum::{
    extract::{Query, Request},
    http::StatusCode,
    response::{IntoResponse, Redirect},
};
use reqwest::Client;
use serde::Deserialize;
use serde_urlencoded;
use std::collections::HashMap;
use tower_sessions_cookie_store::Session;

pub async fn login(request: Request) -> impl IntoResponse {
    // Build redirect_uri based on request headers (to support reverse proxies)
    let proto = request
        .headers()
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("http");
    let host = request
        .headers()
        .get("host")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("localhost");
    let base_url = format!("{}://{}", proto, host);
    let redirect_uri = format!("{}/callback", base_url.trim_end_matches('/'));

    let client_id = match std::env::var("GITHUB_CLIENT_ID") {
        Ok(v) => v,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "GITHUB_CLIENT_ID not set",
            )
                .into_response();
        }
    };

    let url = match reqwest::Url::parse_with_params(
        "https://github.com/login/oauth/authorize",
        &[
            ("client_id", client_id.as_str()),
            ("redirect_uri", redirect_uri.as_str()),
            ("scope", "read:user read:org"),
        ],
    ) {
        Ok(u) => u.to_string(),
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to build authorize URL",
            )
                .into_response();
        }
    };

    Redirect::to(&url).into_response()
}

pub async fn callback(
    Query(query): Query<HashMap<String, String>>,
    session: Session,
    request: Request,
) -> impl IntoResponse {
    let code = match query.get("code") {
        Some(c) => c.clone(),
        None => return (StatusCode::BAD_REQUEST, "Missing `code` query parameter").into_response(),
    };

    let client_id = std::env::var("GITHUB_CLIENT_ID").unwrap_or_default();
    let client_secret = std::env::var("GITHUB_CLIENT_SECRET").unwrap_or_default();
    let github_org = std::env::var("GITHUB_ORG").unwrap_or_default();

    if client_id.is_empty() || client_secret.is_empty() || github_org.is_empty() {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "GITHUB_CLIENT_ID, GITHUB_CLIENT_SECRET or GITHUB_ORG not set",
        )
            .into_response();
    }

    let proto = request
        .headers()
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("http");
    let host = request
        .headers()
        .get("host")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("localhost");
    let base_url = format!("{}://{}", proto, host);
    let redirect_uri = format!("{}/callback", base_url.trim_end_matches('/'));

    let client = Client::new();

    #[derive(Deserialize)]
    struct TokenResponse {
        access_token: Option<String>,
    }

    let params = [
        ("client_id", client_id.as_str()),
        ("client_secret", client_secret.as_str()),
        ("code", code.as_str()),
        ("redirect_uri", redirect_uri.as_str()),
    ];

    let body = match serde_urlencoded::to_string(&params) {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to build token request body: {}", e),
            )
                .into_response();
        }
    };

    let token_res = client
        .post("https://github.com/login/oauth/access_token")
        .header("Accept", "application/json")
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await;

    let token_res = match token_res {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                format!("Failed to request access token: {}", e),
            )
                .into_response();
        }
    };

    let token_data: TokenResponse = match token_res.json().await {
        Ok(j) => j,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                format!("Failed to parse token response: {}", e),
            )
                .into_response();
        }
    };

    let access_token = match token_data.access_token {
        Some(t) => t,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                "Failed to retrieve access token from GitHub.",
            )
                .into_response();
        }
    };

    #[derive(Deserialize)]
    struct User {
        login: String,
    }
    #[derive(Deserialize, Debug)]
    struct Org {
        login: String,
    }

    let user_res = client
        .get("https://api.github.com/user")
        .header("Authorization", format!("Bearer {}", access_token))
        .header("User-Agent", "cr-dash-rs")
        .send()
        .await;

    let user_res = match user_res {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                format!("Failed to fetch user: {}", e),
            )
                .into_response();
        }
    };
    let user: User = match user_res.json().await {
        Ok(u) => u,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                format!("Failed to parse user: {}", e),
            )
                .into_response();
        }
    };

    log::info!("GitHub user: {}", user.login);

    let orgs_res = client
        .get("https://api.github.com/user/orgs")
        .header("Authorization", format!("Bearer {}", access_token))
        .header("User-Agent", "cr-dash-rs")
        .send()
        .await;

    let orgs_res = match orgs_res {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                format!("Failed to fetch orgs: {}", e),
            )
                .into_response();
        }
    };

    let orgs: Vec<Org> = match orgs_res.json().await {
        Ok(o) => o,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                format!("Failed to parse orgs: {}", e),
            )
                .into_response();
        }
    };
    log::info!("GitHub orgs for user {}: {:?}", user.login, orgs);

    let allowed = orgs.iter().any(|o| o.login == github_org);
    if !allowed {
        return (StatusCode::FORBIDDEN, "Access denied").into_response();
    }

    // Store login in session
    session
        .insert("user", user.login.clone())
        .await
        .unwrap_or_else(|e| log::error!("Failed to store session: {}", e));
    Redirect::to("/").into_response()
}
