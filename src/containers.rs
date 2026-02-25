use askama::Template;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};
use tower_sessions_cookie_store::Session;
use serde::{Deserialize};

use std::time::{Duration, Instant};

#[derive(Template)]
#[template(path = "public.html")]
struct Public {}

#[derive(Template)]
#[template(path = "repositories.html")]
struct Repositories {
    user: String,
    repositories: Vec<String>,
}

#[derive(Deserialize, Debug)]
struct Catalog {
    repositories: Vec<String>,
}

#[derive(Default, Debug)]
pub struct CachedCatalog {
    refreshed_at: Option<Instant>,
    repositories: Vec<String>,
}

pub async fn catalog(State(state): State<crate::AppState>, session: Session) -> impl IntoResponse {

    let user = match session.get::<String>("user").await {
        Ok(user) => user,
        Err(err) => {
            log::error!("catalog: Failed to get session: {}", err);
            None
        }
    };

    let Some(user) = user else {
        log::info!("catalog: No user in session");
        let template = Public {};
        return HtmlTemplate(template).into_response();
    };

    log::info!("catalog: User in session: {}", user);

    // Cache catalog for 5 minutes to reduce load on CR
    {
        let mut cached_catalog = state.cached_catalog.lock().unwrap();
        if let Some(refreshed_at) = cached_catalog.refreshed_at {
            if refreshed_at.elapsed() < Duration::from_secs(300) {
                log::info!("Using cached catalog");
                let template = Repositories { user, repositories: cached_catalog.repositories.clone() };
                return HtmlTemplate(template).into_response();
            }
        }
    }

    // Fetch repositories/catalog from CR
    let url = std::env::var("CR_URL").expect("CR_URL must be set");
    let client = reqwest::Client::new();

    let response = client.get(&format!("{url}/v2/_catalog"))
        .basic_auth(std::env::var("CR_USERNAME").expect("CR_USERNAME must be set"), Some(std::env::var("CR_PASSWORD").expect("CR_PASSWORD must be set")))
        .send()
        .await;

    let response = match response {
        Ok(resp) => resp,
        Err(e) => {
            log::error!("catalog: Failed to send request to CR: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to fetch catalog").into_response();
        }
    };

    let catalog: Catalog = match response.json().await {
        Ok(catalog) => catalog,
        Err(e) => {
            log::error!("catalog: Failed to parse response: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to parse catalog response").into_response();
        }
    };
    log::info!("Fetched catalog: {:?}", catalog);

    // Update cache
    {
        let mut cached_catalog = state.cached_catalog.lock().unwrap();
        cached_catalog.refreshed_at = Some(Instant::now());
        cached_catalog.repositories = catalog.repositories.clone();
    }


    let template = Repositories { user, repositories: catalog.repositories };
    HtmlTemplate(template).into_response()
}


#[derive(Deserialize, Debug, Template)]
#[template(path = "tags.html")]
struct Tags {
    repository: String,
    tags: Vec<String>,
}

pub async fn tags(Path(path): Path<String>, session: Session) -> impl IntoResponse {
    let user = match session.get::<String>("user").await {
        Ok(user) => user,
        Err(err) => {
            log::error!("tags: Failed to get session: {}", err);
            None
        }
    };

    let Some(user) = user else {
        log::info!("tags: No user in session");
        let template = Public {};
        return HtmlTemplate(template).into_response();
    };

    log::info!("tags: User in session: {}", user);

    log::info!("Fetching tags for repository: {}", path);
    let url = std::env::var("CR_URL").expect("CR_URL must be set");
    let client = reqwest::Client::new();
    let response = client.get(&format!("{url}/v2/{path}/tags/list"))
        .basic_auth(std::env::var("CR_USERNAME").expect("CR_USERNAME must be set"), Some(std::env::var("CR_PASSWORD").expect("CR_PASSWORD must be set")))
        .send()
        .await
        .expect("Failed to send request to CR");
    
    #[derive(Deserialize, Debug)]
    struct TagsResponse {
        //name: String,
        tags: Vec<String>,
    }
    
    let tags_res: TagsResponse = response.json().await.expect("Failed to parse response");
    log::info!("Fetched tags: {:?}", tags_res);
    // Sort tags alphabetically, in reverse order (newest first)
    let mut tags = tags_res.tags;
    tags.sort_by(|a, b| b.cmp(a));

    HtmlTemplate(Tags { repository: path, tags }).into_response()
}

/******************* Askama -> Axum response ****************/
struct HtmlTemplate<T>(T);

impl<T> IntoResponse for HtmlTemplate<T>
where
    T: Template,
{
    fn into_response(self) -> Response {
        match self.0.render() {
            Ok(html) => Html(html).into_response(),
            Err(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to render template. Error: {err}"),
            )
                .into_response(),
        }
    }
}
