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

    // Cache repositories list for 5 minutes to reduce load on CR
    {
        let cached_catalog = state.cached_catalog.lock().unwrap();
        if let Some(refreshed_at) = cached_catalog.refreshed_at {
            if refreshed_at.elapsed() < Duration::from_secs(300) {
                log::info!("Using cached repositories");
                let template = Repositories { user, repositories: cached_catalog.repositories.clone() };
                return HtmlTemplate(template).into_response();
            }
        }
    }

    // Fetch repositories from CR
    let url = std::env::var("CR_URL").expect("CR_URL must be set");
    let client = reqwest::Client::new();

    let mut request = client.get(&format!("{url}/v2/_catalog"));
    if let (Ok(username), Ok(password)) = (std::env::var("CR_USERNAME"), std::env::var("CR_PASSWORD")) {
        log::debug!("Using basic auth for CR");
        request = request.basic_auth(username, Some(password));
    } else {
        log::debug!("CR_USERNAME or CR_PASSWORD not set, skipping basic auth");
    }

    let response = match request.send().await {
        Ok(resp) => resp,
        Err(e) => {
            log::error!("catalog: Failed to send request to CR: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to fetch repositories").into_response();
        }
    };

    let catalog: Catalog = match response.json().await {
        Ok(catalog) => catalog,
        Err(e) => {
            log::error!("catalog: Failed to parse response: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to parse registry response").into_response();
        }
    };
    log::info!("Fetched catalog/repositories: {:?}", catalog);

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
    message: Option<String>,
    error: Option<String>,
}

#[derive(Deserialize, Debug, Default)]
pub struct DeleteTagsForm {
    #[serde(default, deserialize_with = "deserialize_string_or_vec")]
    pub tags: Vec<String>,
}

fn deserialize_string_or_vec<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct StringOrVec;

    impl<'de> serde::de::Visitor<'de> for StringOrVec {
        type Value = Vec<String>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a string or list of strings")
        }


        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(vec![value.to_string()])
        }

        fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(vec![value])
        }

        fn visit_seq<S>(self, mut seq: S) -> Result<Self::Value, S::Error>
        where
            S: serde::de::SeqAccess<'de>,
        {
            let mut vec = Vec::new();
            while let Some(elem) = seq.next_element()? {
                vec.push(elem);
            }
            Ok(vec)
        }
    }

    deserializer.deserialize_any(StringOrVec)
}


async fn fetch_tags_list(path: &str) -> Result<Vec<String>, String> {
    let url = std::env::var("CR_URL").map_err(|_| "CR_URL must be set".to_string())?;
    let client = reqwest::Client::new();

    let mut request = client.get(&format!("{url}/v2/{path}/tags/list"));
    if let (Ok(username), Ok(password)) = (std::env::var("CR_USERNAME"), std::env::var("CR_PASSWORD")) {
        log::debug!("Using basic auth for CR");
        request = request.basic_auth(username, Some(password));
    } else {
        log::debug!("CR_USERNAME or CR_PASSWORD not set, skipping basic auth");
    }

    let response = request.send().await.map_err(|e| format!("Failed to send request to CR: {e}"))?;

    #[derive(Deserialize, Debug)]
    struct TagsResponse {
        // name: String,
        tags: Option<Vec<String>>,
    }

    let status = response.status();
    if !status.is_success() {
        return Err(format!("Registry returned HTTP status {status}"));
    }

    let tags_res: TagsResponse = response.json().await.map_err(|e| format!("Failed to parse registry response: {e}"))?;
    let mut tags = tags_res.tags.unwrap_or_default();
    tags.sort_by(|a, b| b.cmp(a));
    Ok(tags)
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

    match fetch_tags_list(&path).await {
        Ok(tags) => HtmlTemplate(Tags {
            repository: path,
            tags,
            message: None,
            error: None,
        })
        .into_response(),
        Err(e) => {
            log::error!("tags: Error fetching tags for {}: {}", path, e);
            (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to fetch tags: {}", e)).into_response()
        }
    }
}

pub async fn delete_tags(
    Path(path): Path<String>,
    session: Session,
    body: String,
) -> impl IntoResponse {
    let form: DeleteTagsForm = match serde_html_form::from_str(&body) {
        Ok(f) => f,
        Err(e) => {
            log::error!("delete_tags: Failed to parse form body string '{body}': {e}");
            DeleteTagsForm::default()
        }
    };

    let user = match session.get::<String>("user").await {
        Ok(user) => user,
        Err(err) => {
            log::error!("delete_tags: Failed to get session: {}", err);
            None
        }
    };

    let Some(_user) = user else {
        log::info!("delete_tags: No user in session");
        let template = Public {};
        return HtmlTemplate(template).into_response();
    };

    if form.tags.is_empty() {
        return render_tags_with_feedback(path, None, Some("No tags selected for deletion.".to_string())).await;
    }

    let url = match std::env::var("CR_URL") {
        Ok(u) => u,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "CR_URL must be set").into_response(),
    };

    let client = reqwest::Client::new();
    let auth = match (std::env::var("CR_USERNAME"), std::env::var("CR_PASSWORD")) {
        (Ok(u), Ok(p)) => Some((u, p)),
        _ => None,
    };

    let mut deleted = Vec::new();
    let mut failed = Vec::new();

    for tag in &form.tags {
        // Step 1: GET/HEAD manifest to resolve Docker-Content-Digest
        let manifest_url = format!("{url}/v2/{path}/manifests/{tag}");
        let mut head_req = client
            .head(&manifest_url)
            .header(
                "Accept",
                "application/vnd.docker.distribution.manifest.v2+json, application/vnd.oci.image.manifest.v1+json, application/vnd.docker.distribution.manifest.list.v2+json, application/vnd.oci.image.index.v1+json",
            );

        if let Some((username, password)) = &auth {
            head_req = head_req.basic_auth(username, Some(password));
        }

        let mut digest_to_delete = None;
        if let Ok(resp) = head_req.send().await {
            if let Some(digest_header) = resp.headers().get("docker-content-digest") {
                if let Ok(d) = digest_header.to_str() {
                    digest_to_delete = Some(d.to_string());
                }
            }
        }

        let reference = digest_to_delete.as_deref().unwrap_or(tag.as_str());
        let delete_url = format!("{url}/v2/{path}/manifests/{reference}");
        let mut del_req = client.delete(&delete_url);

        if let Some((username, password)) = &auth {
            del_req = del_req.basic_auth(username, Some(password));
        }

        match del_req.send().await {
            Ok(resp) => {
                if resp.status().is_success() {
                    log::info!("Successfully deleted image {} tag {} (ref: {})", path, tag, reference);
                    deleted.push(tag.clone());
                } else {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    log::error!("Failed to delete {} tag {}: {} - {}", path, tag, status, body);
                    if status == StatusCode::METHOD_NOT_ALLOWED {
                        failed.push(format!("{tag}: deletion disabled on registry (405 Method Not Allowed)"));
                    } else {
                        failed.push(format!("{tag}: registry returned {status}"));
                    }
                }
            }
            Err(e) => {
                log::error!("Error sending delete request for {} tag {}: {}", path, tag, e);
                failed.push(format!("{tag}: request error ({e})"));
            }
        }
    }

    let message = if !deleted.is_empty() {
        Some(format!("Successfully deleted tag(s): {}", deleted.join(", ")))
    } else {
        None
    };

    let error = if !failed.is_empty() {
        Some(format!("Failed to delete: {}", failed.join("; ")))
    } else {
        None
    };

    render_tags_with_feedback(path, message, error).await
}

async fn render_tags_with_feedback(
    path: String,
    message: Option<String>,
    error: Option<String>,
) -> Response {
    match fetch_tags_list(&path).await {
        Ok(tags) => HtmlTemplate(Tags {
            repository: path,
            tags,
            message,
            error,
        })
        .into_response(),
        Err(err_msg) => {
            let err_combined = match error {
                Some(prev_err) => format!("{prev_err}; {err_msg}"),
                None => err_msg,
            };
            HtmlTemplate(Tags {
                repository: path,
                tags: Vec::new(),
                message,
                error: Some(err_combined),
            })
            .into_response()
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_delete_tags_form_deserialization() {
        // Multiple tags
        let form_data = "tags=v1.0.0&tags=v1.0.1";
        let parsed: DeleteTagsForm = serde_html_form::from_str(form_data).unwrap();
        assert_eq!(parsed.tags, vec!["v1.0.0", "v1.0.1"]);

        // Single tag
        let single_data = "tags=2.0.1";
        let parsed_single: DeleteTagsForm = serde_html_form::from_str(single_data).unwrap();
        assert_eq!(parsed_single.tags, vec!["2.0.1"]);

        // Empty form
        let empty_form_data = "";
        let parsed_empty: DeleteTagsForm = serde_html_form::from_str(empty_form_data).unwrap();
        assert!(parsed_empty.tags.is_empty());
    }



    #[test]
    fn test_tags_template_rendering() {
        let template = Tags {
            repository: "my-app".to_string(),
            tags: vec!["v2.0.0".to_string(), "v1.0.0".to_string()],
            message: Some("Successfully deleted tag(s): v0.9.0".to_string()),
            error: None,
        };
        let rendered = template.render().unwrap();
        assert!(rendered.contains("Tags for <code>my-app</code>"));
        assert!(rendered.contains("v2.0.0"));
        assert!(rendered.contains("v1.0.0"));
        assert!(rendered.contains("Successfully deleted tag(s): v0.9.0"));
        assert!(rendered.contains("Delete Selected"));
    }
}


