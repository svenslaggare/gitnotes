use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use thiserror::Error;

use serde_json::json;
use serde::Deserialize;

use tokio::sync::Notify;

use axum::response::{Html, IntoResponse, Response};
use axum::{Json, Router};
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use axum::extract::{Query, State};

use tower_http::services::ServeDir;

use askama::{Template};

pub async fn launch(port: u16, path: &Path) {
    let state = Arc::new(WebServerState::new(path.to_owned()));
    let app = Router::new()
        .nest_service("/content", ServeDir::new("webeditor/static"))
        .route("/", get(index))
        .route("/api/stop", post(stop))
        .route("/api/content", get(get_content))
        .route("/api/content", put(save_content))
        .with_state(state.clone())
        ;

    let address = SocketAddr::new(Ipv4Addr::from_str(&"127.0.0.1").unwrap().into(), port);
    println!("Listening on {}", address);

    tokio::process::Command::new("xdg-open")
        .arg(format!("http://{}", address))
        .spawn().unwrap()
        .wait().await.unwrap();

    tokio::select! {
        result = axum::Server::bind(&address).serve(app.into_make_service()) => {
            result.unwrap();
        }
        _ = state.notify.notified() => {
            return;
        }
    }
}

pub fn launch_sync(port: u16, path: &Path) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(launch(port, path));
}

struct WebServerState {
    path: PathBuf,
    notify: Notify
}

impl WebServerState {
    pub fn new(path: PathBuf) -> WebServerState {
        WebServerState {
            path,
            notify: Notify::new()
        }
    }
}

#[derive(Error, Debug)]
enum WebServerError {
    #[error("Expected query parameter '{0}'")]
    ExpectedQueryParameter(String),

    #[error("{0}")]
    IO(std::io::Error)
}

impl From<std::io::Error> for WebServerError {
    fn from(err: std::io::Error) -> Self {
        WebServerError::IO(err)
    }
}

type WebServerResult<T> = Result<T, WebServerError>;

impl IntoResponse for WebServerError {
    fn into_response(self) -> Response {
        let (status_code, error_message) = (StatusCode::BAD_REQUEST, self.to_string());
        with_response_code(
            Json(
                json!({
                    "message": error_message
                })
            ).into_response(),
            status_code
        )
    }
}

#[derive(Template)]
#[template(path="webEditor.html")]
struct AppTemplate {
    file_path: String
}

async fn index(State(state): State<Arc<WebServerState>>) -> Response {
    let template = AppTemplate {
        file_path: state.path.to_str().unwrap().to_owned(),
    };

    Html(template.render().unwrap()).into_response()
}

async fn stop(State(state): State<Arc<WebServerState>>) -> WebServerResult<Response> {
    state.notify.notify_one();
    Ok(Json(json!({})).into_response())
}

async fn get_content(Query(parameters): Query<HashMap<String, String>>) -> WebServerResult<Response> {
    let path = parameters.get("path").ok_or_else(|| WebServerError::ExpectedQueryParameter("path".to_owned()))?;

    Ok(
        Json(
            json!({
                "content": std::fs::read_to_string(path)?
            })
        ).into_response()
    )
}

#[derive(Deserialize)]
struct SaveContent {
    path: PathBuf,
    content: String
}

async fn save_content(Json(input): Json<SaveContent>) -> WebServerResult<Response> {
    std::fs::write(input.path, input.content)?;
    Ok(Json(json!({})).into_response())
}

fn with_response_code(mut response: Response, code: StatusCode) -> Response {
    *response.status_mut() = code;
    response
}