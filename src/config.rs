use std::path::{Path, PathBuf};

use serde::{Serialize, Deserialize};
use crate::helpers::{base_dir, io_error};

pub fn config_path() -> PathBuf {
    base_dir().join("config.toml")
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FileConfig {
    pub repository: PathBuf,
    pub editor: Option<String>,
}

impl FileConfig {
    pub fn new(repository: &Path) -> FileConfig {
        FileConfig {
            repository: repository.to_owned(),
            editor: None,
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
}

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub repository: PathBuf,
    pub user_name_and_email: (String, String),
    pub editor: String,
}

impl Config {
    pub fn from_env(file_config: FileConfig) -> Config {
        Config {
            repository: std::env::var("GITNOTES_REPOSITORY").map(|path| Path::new(&path).to_owned()).unwrap_or_else(|_| file_config.repository),
            user_name_and_email: get_user_name_and_email(),
            editor: std::env::var("GITNOTES_EDITOR").unwrap_or_else(|_| file_config.editor.unwrap_or("code".to_owned())),
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