use std::path::{Path, PathBuf};
use std::str::FromStr;
use home::home_dir;

use serde::{Serialize, Deserialize};

use crate::helpers::{base_dir, io_error};
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
    pub use_real: bool,
    pub real_base_dir: Option<PathBuf>
}

impl FileConfig {
    pub fn new(repository: &Path) -> FileConfig {
        FileConfig {
            repository: repository.to_owned(),
            editor: None,
            snippet: None,
            use_real: false,
            real_base_dir: None
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
            "use_real" => {
                self.use_real = bool::from_str(value).map_err(|err| err.to_string())?;
            }
            "real_base_dir" => {
                self.real_base_dir = Some(Path::new(value).to_owned());
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
    pub use_real: bool,
    pub real_base_dir: Option<PathBuf>
}

impl Config {
    pub fn from_env(file_config: FileConfig) -> Config {
        Config {
            repository: std::env::var("GITNOTES_REPOSITORY").map(|path| Path::new(&path).to_owned()).unwrap_or_else(|_| file_config.repository),
            user_name_and_email: get_user_name_and_email(),
            editor: std::env::var("GITNOTES_EDITOR").unwrap_or_else(|_| file_config.editor.unwrap_or("web-editor".to_owned())),
            snippet: file_config.snippet,
            use_real: file_config.use_real,
            real_base_dir: file_config.real_base_dir.or_else(|| home_dir())
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