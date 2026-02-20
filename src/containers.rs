use askama::Template;
use axum::{
    extract::Path,
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};
use tower_sessions_cookie_store::Session;
use serde::{Deserialize};

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


pub async fn catalog(session: Session) -> impl IntoResponse {

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

    let url = std::env::var("CR_URL").expect("CR_URL must be set");
    let client = reqwest::Client::new();
    let response = client.get(&format!("{url}/v2/_catalog"))
        .basic_auth(std::env::var("CR_USERNAME").expect("CR_USERNAME must be set"), Some(std::env::var("CR_PASSWORD").expect("CR_PASSWORD must be set")))
        .send()
        .await
        .expect("Failed to send request to CR");
    let catalog: Catalog = response.json().await.expect("Failed to parse response");
    log::info!("Fetched catalog: {:?}", catalog);
    let template = Repositories { user, repositories: catalog.repositories };
    HtmlTemplate(template).into_response()
}


#[derive(Deserialize, Debug, Template)]
#[template(path = "tags.html")]
struct Tags {
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
    let tags: Tags = response.json().await.expect("Failed to parse response");
    log::info!("Fetched tags: {:?}", tags);
    HtmlTemplate(tags).into_response()
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
