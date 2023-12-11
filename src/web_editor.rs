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
use axum::http::{HeaderMap, Request, StatusCode};
use axum::routing::{get, post, put};
use axum::extract::{Path as AxumPath, Query, State};

use tower_http::services::{ServeDir, ServeFile};

use askama::{Template};
use axum::body::Body;
use comrak::nodes::NodeValue;

use crate::command::CommandError;
use crate::config::SnippetFileConfig;
use crate::markdown;
use crate::snippets::{SnippetError, SnippetRunnerManger};

#[derive(Debug, Serialize, Deserialize)]
pub struct WebEditorConfig {
    pub port: u16,
    pub launch_web_view: bool,
    pub is_read_only: bool,
    pub repository_path: Option<PathBuf>,
    pub snippet_config: Option<SnippetFileConfig>
}

impl Default for WebEditorConfig {
    fn default() -> Self {
        WebEditorConfig {
            port: 9000,
            launch_web_view: default_launch_web_view(),
            is_read_only: false,
            repository_path: None,
            snippet_config: None
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

    let state = Arc::new(WebServerState::new(
        path.to_owned(),
        config.is_read_only,
        config.repository_path.clone(),
        SnippetRunnerManger::from_config(config.snippet_config.as_ref()).unwrap()
    ));

    let app = Router::new()
        .nest_service("/content", ServeDir::new(content_dir))
        .route("/", get(index))
        .route("/api/stop", post(stop))
        .route("/api/content", get(get_content))
        .route("/api/content", put(save_content))
        .route("/api/run-snippet", post(run_snippet))
        .route("/local/*path", get(get_local_file))
        .route("/resource/*path", get(get_resource_file))
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
    notify: Notify,
    is_read_only: bool,
    repository_path: Option<PathBuf>,
    snippet_runner_manager: SnippetRunnerManger
}

impl WebServerState {
    pub fn new(path: PathBuf,
               is_read_only: bool,
               repository_path: Option<PathBuf>,
               snippet_runner_manager: SnippetRunnerManger) -> WebServerState {
        WebServerState {
            path,
            notify: Notify::new(),
            is_read_only,
            repository_path,
            snippet_runner_manager
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
    file_path: String,
    is_read_only: bool
}

async fn index(State(state): State<Arc<WebServerState>>) -> Response {
    let template = AppTemplate {
        time: Local::now().timestamp(),
        file_path: state.path.to_str().unwrap().to_owned(),
        is_read_only: state.is_read_only
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

#[derive(Deserialize)]
struct RunSnippet {
    content: String
}

async fn run_snippet(State(state): State<Arc<WebServerState>>, Json(input): Json<RunSnippet>) -> WebServerResult<Response> {
    let arena = markdown::storage();
    let root = markdown::parse(&arena, &input.content);

    let mut snippet_output = String::new();
    markdown::visit_code_blocks::<CommandError, _>(
        &root,
        |current_node| {
            if let NodeValue::CodeBlock(ref block) = current_node.data.borrow().value {
                let snippet_result = state.snippet_runner_manager.run(
                    &block.info,
                    &block.literal
                );

                match snippet_result {
                    Ok(output_stdout) => {
                        snippet_output += &output_stdout;
                    }
                    Err(SnippetError::Execution { output, .. }) => {
                        snippet_output += &output;
                    }
                    Err(err) => {
                        snippet_output += &err.to_string();
                        snippet_output.push('\n');
                    }
                };
            }

            Ok(())
        },
        true,
        false
    ).unwrap();

    Ok(Json(json!({ "output": snippet_output })).into_response())
}

async fn get_local_file(headers: HeaderMap, AxumPath(path): AxumPath<String>) -> Response {
    serve_file(headers, Path::new(&path)).await
}

async fn get_resource_file(State(state): State<Arc<WebServerState>>,
                           headers: HeaderMap,
                           AxumPath(path): AxumPath<String>) -> Response {
    if let Some(repository_path) = state.repository_path.as_ref() {
        serve_file(headers, &repository_path.join("resources").join(&path)).await
    } else {
        with_response_code(
            "Repository path not set.".into_response(),
            StatusCode::BAD_REQUEST
        )
    }
}

async fn serve_file(headers: HeaderMap, path: &Path) -> Response {
    let mut request = Request::new(Body::empty());
    *request.headers_mut() = headers;

    if let Ok(result) = ServeFile::new(Path::new(&path)).try_call(request).await {
        result.into_response()
    } else {
        with_response_code(
            "File not found.".into_response(),
            StatusCode::NOT_FOUND
        )
    }
}

fn with_response_code(mut response: Response, code: StatusCode) -> Response {
    *response.status_mut() = code;
    response
}