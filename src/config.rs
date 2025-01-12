use std::path::{Path, PathBuf};
use home::home_dir;

use serde::{Serialize, Deserialize};

use crate::helpers::{base_dir, io_error};
use crate::model::RESOURCES_DIR;
use crate::snippets::{PythonSnippetRunnerConfig, RustSnippetRunnerConfig};

pub fn config_path() -> PathBuf {
    base_dir().join("config.toml")
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileConfig {
    pub repository: PathBuf,
    pub editor: Option<String>,
    pub snippet: Option<SnippetFileConfig>,
    pub base_dir: Option<PathBuf>,
    pub sync_default_branch: Option<String>,
    pub sync_default_remote: Option<String>
}

impl FileConfig {
    pub fn new(repository: &Path) -> FileConfig {
        FileConfig {
            repository: repository.to_owned(),
            editor: None,
            snippet: None,
            base_dir: None,
            sync_default_branch: None,
            sync_default_remote: None
        }
    }

    pub fn load(path: &Path) -> std::io::Result<FileConfig> {
        let content = std::fs::read_to_string(path)?;
        toml::from_str(&content).map_err(|err| io_error(err))
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let toml = toml::to_string(self).map_err(|err| io_error(err))?;
        std::fs::write(path, toml)
    }

    pub fn change(&mut self, key: &str, value: &str) -> Result<(), String> {
        match key {
            "repository" => {
                self.repository = Path::new(value).to_path_buf();
            }
            "editor" => {
                self.editor = Some(value.to_owned());
            }
            "base_dir" => {
                self.base_dir = Some(Path::new(value).to_owned());
            }
            "sync_default_branch" => {
                self.sync_default_branch = Some(value.to_owned());
            }
            "sync_default_remote" => {
                self.sync_default_remote = Some(value.to_owned());
            }
            _ => {
                return Err(format!("Undefined key: {}", key));
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnippetFileConfig {
    pub python: Option<PythonSnippetRunnerConfig>,
    pub cpp: Option<RustSnippetRunnerConfig>,
    pub rust: Option<RustSnippetRunnerConfig>
}

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub repository: PathBuf,
    pub user_name_and_email: (String, String),
    pub editor: String,
    pub snippet: Option<SnippetFileConfig>,
    pub base_dir: Option<PathBuf>,
    pub use_working_dir: bool,
    pub allow_stdin: bool,
    pub sync_default_branch: String,
    pub sync_default_remote: String
}

impl Config {
    pub fn from_env(file_config: FileConfig) -> Config {
        Config {
            repository: std::env::var("GITNOTES_REPOSITORY").map(|path| Path::new(&path).to_owned()).unwrap_or_else(|_| file_config.repository),
            user_name_and_email: get_user_name_and_email(),
            editor: std::env::var("GITNOTES_EDITOR").unwrap_or_else(|_| file_config.editor.unwrap_or("web-editor".to_owned())),
            snippet: file_config.snippet,
            base_dir: file_config.base_dir.or_else(|| home_dir()),
            use_working_dir: true,
            allow_stdin: true,
            sync_default_branch: file_config.sync_default_branch.unwrap_or("master".to_owned()),
            sync_default_remote: file_config.sync_default_remote.unwrap_or("origin".to_owned())
        }
    }

    pub fn load(path: &Path) -> std::io::Result<Config> {
        let config = FileConfig::load(&path)?;
        Ok(Config::from_env(config))
    }

    pub fn print(&self) {
        println!("Repository: {}", self.repository.to_str().unwrap());
        println!("User name: {}, email: {}", self.user_name_and_email.0, self.user_name_and_email.1);
        println!("Editor: {}", self.editor);
        println!("Snippet: {}", self.snippet.is_some());
        println!("Base dir: {}", self.base_dir.as_ref().map(|x| x.to_str().unwrap()).unwrap_or("N/A"));
    }

    pub fn resources_dir(&self) -> PathBuf {
        self.repository.join(RESOURCES_DIR)
    }
}

fn get_user_name_and_email() -> (String, String) {
    if let Ok(config) =  git2::Config::open_default() {
        match (config.get_string("user.name"), config.get_string("user.email")) {
            (Ok(name), Ok(email)) => { return (name, email); },
            _ => {}
        }
    }

    ("unknown".to_owned(), "unknown".to_owned())
}