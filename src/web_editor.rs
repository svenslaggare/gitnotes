use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use chrono::{Local};
use thiserror::Error;

use serde_json::json;
use serde::{Deserialize, Serialize};

use tokio::sync::Notify;

use axum::response::{Html, IntoResponse, Response};
use axum::{Json, Router};
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use axum::extract::{Query, State};

use tower_http::services::ServeDir;

use askama::{Template};

#[derive(Debug, Serialize, Deserialize)]
pub struct WebEditorConfig {
    pub port: u16,
    pub launch_web_view: bool
}

impl Default for WebEditorConfig {
    fn default() -> Self {
        WebEditorConfig {
            port: 9000,
            launch_web_view: default_launch_web_view()
        }
    }
}

#[cfg(feature="webview")]
fn default_launch_web_view() -> bool {
    true
}

#[cfg(not(feature="webview"))]
fn default_launch_web_view() -> bool {
    false
}

pub async fn launch(config: WebEditorConfig, path: &Path) {
    let mut content_dir = Path::new("webeditor/static");
    if !content_dir.exists() {
        content_dir = Path::new("/etc/gitnotes/static");
    }

    let state = Arc::new(WebServerState::new(path.to_owned()));
    let app = Router::new()
        .nest_service("/content", ServeDir::new(content_dir))
        .route("/", get(index))
        .route("/api/stop", post(stop))
        .route("/api/content", get(get_content))
        .route("/api/content", put(save_content))
        .with_state(state.clone())
        ;

    let address = SocketAddr::new(Ipv4Addr::from_str(&"127.0.0.1").unwrap().into(), config.port);
    let web_address = format!("http://{}", address);
    println!("Opening file '{}' with web editor available at {}.", path.to_str().unwrap(), web_address);

    if config.launch_web_view {
        launch_web_view(state.clone(), &config);
    } else {
        open::that(web_address).unwrap();
    }

    tokio::select! {
        result = axum::Server::bind(&address).serve(app.into_make_service()) => {
            result.unwrap();
        }
        _ = state.notify.notified() => {
            return;
        }
    }
}

pub fn launch_sync(config: WebEditorConfig, path: &Path) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(launch(config, path));
}

#[cfg(feature="webview")]
fn launch_web_view(state: Arc<WebServerState>, config: &WebEditorConfig) {
    let port = config.port;
    tokio::task::spawn_blocking(move || {
        web_view::builder()
            .title("WebEditor")
            .content(web_view::Content::Url(format!("http://localhost:{}", port)))
            .size(1440, 960)
            .resizable(true)
            .debug(true)
            .user_data(())
            .invoke_handler(|webview, arg| {
                if arg == "exit" {
                    webview.exit();
                }

                Ok(())
            })
            .run()
            .unwrap();
        state.notify.notify_one();
    });
}

#[cfg(not(feature="webview"))]
fn launch_web_view(_state: Arc<WebServerState>, _config: &WebEditorConfig) {
    panic!("Webview feature not compiled - compile with --features webview.");
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
    time: i64,
    file_path: String
}

async fn index(State(state): State<Arc<WebServerState>>) -> Response {
    let template = AppTemplate {
        time: Local::now().timestamp(),
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
    std::fs::write(&input.path, input.content)?;
    println!("Saved content for '{}'.",  input.path.to_str().unwrap());
    Ok(Json(json!({})).into_response())
}

fn with_response_code(mut response: Response, code: StatusCode) -> Response {
    *response.status_mut() = code;
    response
}