use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr};
use std::ops::DerefMut;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use chrono::Local;
use thiserror::Error;

use serde_json::json;
use serde::{Deserialize, Serialize};

use tokio::sync::{Mutex, Notify};
use tokio::signal;

use axum::response::{Html, IntoResponse, Response};
use axum::{Json, Router};
use axum::http::{HeaderMap, Request, StatusCode};
use axum::routing::{get, post, put};
use axum::extract::{DefaultBodyLimit, Multipart, Path as AxumPath, Query, State};

use tower_http::services::{ServeDir, ServeFile};

use askama::Template;
use axum::body::Body;

use crate::config::SnippetFileConfig;
use crate::{command, markdown};
use crate::editor::EditorOutput;
use crate::model::RESOURCES_DIR;
use crate::snippets::SnippetRunnerManger;

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum AccessMode {
    Read,
    ReadWrite
}

impl Default for AccessMode {
    fn default() -> Self {
        Self::ReadWrite
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WebEditorConfig {
    pub port: u16,
    pub access_mode: AccessMode,
    pub is_standalone: bool,
    pub snippet_config: Option<SnippetFileConfig>
}

impl Default for WebEditorConfig {
    fn default() -> Self {
        WebEditorConfig {
            port: 9000,
            access_mode: AccessMode::default(),
            is_standalone: false,
            snippet_config: None
        }
    }
}

pub struct WebEditorInput {
    pub path: PathBuf,
    pub display_path: Option<PathBuf>,
    pub repository_path: Option<PathBuf>
}

impl WebEditorInput {
    pub fn from_path(path: &Path) -> WebEditorInput {
        WebEditorInput {
            path: path.to_owned(),
            display_path: None,
            repository_path: None
        }
    }
}

pub async fn launch(config: WebEditorConfig, input: WebEditorInput) -> EditorOutput {
    let mut content_dir = Path::new("webeditor/static");
    if !content_dir.exists() {
        content_dir = Path::new("/etc/gitnotes/static");
    }

    let state = Arc::new(WebServerState::new(
        input.path.clone(),
        input.display_path.unwrap_or(input.path.clone()),
        config.access_mode,
        config.is_standalone,
        input.repository_path.clone(),
        SnippetRunnerManger::from_config(config.snippet_config.as_ref()).unwrap()
    ));

    let app = Router::new()
        .nest_service("/content", ServeDir::new(content_dir))
        .route("/", get(index))
        .route("/api/stop", post(stop))
        .route("/api/content", get(get_content))
        .route("/api/content", put(save_content))
        .route("/api/run-snippet", post(run_snippet))
        .route("/api/add-resource", post(add_resource))
        .route("/local/*path", get(get_local_file))
        .route("/resource/*path", get(get_resource_file))
        .with_state(state.clone())
        .layer(DefaultBodyLimit::max(10 * 1024 * 1024))
        ;

    let address = SocketAddr::new(Ipv4Addr::from_str(&"127.0.0.1").unwrap().into(), config.port);
    let web_address = format!("http://{}", address);
    println!("Opening file '{}' with web editor available at {}.", input.path.to_str().unwrap(), web_address);

    open::that(web_address).unwrap();

    tokio::select! {
        result = axum::Server::bind(&address).serve(app.into_make_service()) => {
            result.unwrap();
            EditorOutput::default()
        }
        _ = signal::ctrl_c() => {
            EditorOutput::default()
        }
        _ = state.notify.notified() => {
            EditorOutput {
                added_resources: std::mem::take(state.added_resources.lock().await.deref_mut())
            }
        }
    }
}

pub fn launch_sync(config: WebEditorConfig, input: WebEditorInput) -> EditorOutput {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(launch(config, input))
}

struct WebServerState {
    path: PathBuf,
    display_path: PathBuf,
    notify: Notify,
    access_mode: AccessMode,
    is_standalone: bool,
    repository_path: Option<PathBuf>,
    snippet_runner_manager: SnippetRunnerManger,
    added_resources: Mutex<Vec<PathBuf>>
}

impl WebServerState {
    pub fn new(path: PathBuf,
               display_path: PathBuf,
               access_mode: AccessMode,
               is_standalone: bool,
               repository_path: Option<PathBuf>,
               snippet_runner_manager: SnippetRunnerManger) -> WebServerState {
        WebServerState {
            path,
            display_path,
            notify: Notify::new(),
            access_mode,
            is_standalone,
            repository_path,
            snippet_runner_manager,
            added_resources: Mutex::new(Vec::new())
        }
    }
}

#[derive(Error, Debug)]
enum WebServerError {
    #[error("Expected query parameter '{0}'")]
    ExpectedQueryParameter(String),

    #[error("{0}")]
    Multipart(axum::extract::multipart::MultipartError),

    #[error("{0}")]
    IO(std::io::Error)
}

impl From<axum::extract::multipart::MultipartError> for WebServerError {
    fn from(err: axum::extract::multipart::MultipartError) -> Self {
        WebServerError::Multipart(err)
    }
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
    display_file_path: String,
    is_read_only: bool,
    is_standalone: bool
}

async fn index(State(state): State<Arc<WebServerState>>) -> Response {
    let template = AppTemplate {
        time: Local::now().timestamp(),
        file_path: state.path.to_str().unwrap().to_owned(),
        display_file_path: state.display_path.to_str().unwrap().to_owned(),
        is_read_only: state.access_mode == AccessMode::Read,
        is_standalone: state.is_standalone
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

async fn save_content(State(state): State<Arc<WebServerState>>, Json(input): Json<SaveContent>) -> WebServerResult<Response> {
    if state.access_mode == AccessMode::ReadWrite {
        std::fs::write(&input.path, input.content)?;
        println!("Saved content for '{}'.", input.path.to_str().unwrap());
        Ok(Json(json!({})).into_response())
    } else {
        Ok(
            with_response_code(
                "File is read only".into_response(),
                StatusCode::BAD_REQUEST
            )
        )
    }
}

#[derive(Deserialize)]
struct RunSnippet {
    content: String
}

async fn run_snippet(State(state): State<Arc<WebServerState>>, Json(input): Json<RunSnippet>) -> WebServerResult<Response> {
    let arena = markdown::storage();

    let mut snippet_output = String::new();
    let result = command::run_snippet(
        &state.snippet_runner_manager,
        &arena,
        &input.content,
        |text| { snippet_output += text }
    );

    let mut new_content = None;
    match result {
        Ok(root) => {
            new_content = markdown::ast_to_string(&root).ok();
        }
        Err(err) => {
            snippet_output += &err.to_string();
            snippet_output.push('\n');
        }
    }

    Ok(
        Json(json!({
            "output": snippet_output,
            "newContent": new_content
        })).into_response()
    )
}

async fn add_resource(State(state): State<Arc<WebServerState>>,
                      mut multipart: Multipart) -> WebServerResult<Response> {
    if let Some(repository_path) = state.repository_path.as_ref() {
        while let Some(field) = multipart.next_field().await? {
            let filename = field.file_name().unwrap_or("file.bin").to_owned();
            let data = field.bytes().await?;

            println!("Adding resource: {} ({} bytes)", filename, data.len());
            let path = repository_path.join(RESOURCES_DIR).join(&filename);
            std::fs::write(path, data)?;
            state.added_resources.lock().await.push(Path::new(&filename).to_owned())
        }
    }

    Ok("".into_response())
}

async fn get_local_file(headers: HeaderMap, AxumPath(path): AxumPath<String>) -> Response {
    serve_file(headers, Path::new(&path)).await
}

async fn get_resource_file(State(state): State<Arc<WebServerState>>,
                           headers: HeaderMap,
                           AxumPath(path): AxumPath<String>) -> Response {
    if let Some(repository_path) = state.repository_path.as_ref() {
        serve_file(headers, &repository_path.join(RESOURCES_DIR).join(&path)).await
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
