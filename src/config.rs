use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub repository: PathBuf,
    pub user_name_and_email: (String, String),
    pub editor: String,
}

impl Config {
    pub fn from_env(repository: &Path) -> Config {
        Config {
            repository: repository.to_owned(),
            user_name_and_email: get_user_name_and_email(),
            editor: std::env::var("GITNOTES_EDITOR").unwrap_or_else(|_| "code".to_owned()),
        }
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