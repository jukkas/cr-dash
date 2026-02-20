use askama::Template;
use axum::{
    extract::Path,
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};
use serde::{Deserialize};

#[derive(Deserialize, Debug)]
struct Catalog {
    repositories: Vec<String>,
}

#[derive(Template)]
#[template(path = "repositories.html")]
struct RepositoriesTemplate {
    repositories: Vec<String>,
}

pub async fn catalog() -> impl IntoResponse {
    let url = std::env::var("CR_URL").expect("CR_URL must be set");
    let client = reqwest::Client::new();
    let response = client.get(&format!("{url}/v2/_catalog"))
        .basic_auth(std::env::var("CR_USERNAME").expect("CR_USERNAME must be set"), Some(std::env::var("CR_PASSWORD").expect("CR_PASSWORD must be set")))
        .send()
        .await
        .expect("Failed to send request to CR");
    let catalog: Catalog = response.json().await.expect("Failed to parse response");
    log::info!("Fetched catalog: {:?}", catalog);
    let template = RepositoriesTemplate { repositories: catalog.repositories };
    HtmlTemplate(template)
}


#[derive(Deserialize, Debug, Template)]
#[template(path = "tags.html")]
struct Tags {
    tags: Vec<String>,
}

pub async fn tags(Path(path): Path<String>) -> impl IntoResponse {
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
    HtmlTemplate(tags)
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
