use std::path::Path;

use structopt::StructOpt;
use structopt::clap::Shell;

mod config;
mod helpers;
mod model;
mod querying;
mod command;
mod markdown;
mod snippets;
mod editor;
mod web_editor;
mod tags;
mod interactive;
mod app;
mod git_helpers;

#[cfg(test)]
mod app_tests;

use crate::app::{AppError, App, InputCommand, MainInputCommand};
use crate::config::{Config, config_path, FileConfig};
use crate::helpers::base_dir;
use crate::web_editor::{AccessMode, WebEditorConfig, WebEditorInput};

fn main() {
    if generate_completions() {
        return;
    }

    let mut main_command = MainInputCommand::from_args();
    if let Some(input_command) = std::mem::take(&mut main_command.command) {
        if let Err(err) = run(input_command, main_command) {
            println!("{}.", err.to_string());
            std::process::exit(1);
        }
    } else  {
        if let Err(err) = interactive::run(main_command) {
            println!("{}.", err.to_string());
            std::process::exit(1);
        }
    }
}

fn generate_completions() -> bool {
    if std::env::args().skip(1).next() == Some("generate-completions".to_owned()) {
        let output_dir = "completions";
        std::fs::create_dir_all(output_dir).unwrap();
        MainInputCommand::clap().gen_completions("gitnotes", Shell::Bash, output_dir);
        true
    } else {
        false
    }
}

fn run(input_command: InputCommand, main_input_command: MainInputCommand) -> Result<(), AppError> {
    let config_path = config_path();
    match input_command {
        InputCommand::Initialize { .. } => {
            run_init(&config_path, input_command)
        }
        InputCommand::WebEditor { path, port, is_read_only } => {
            let mut config = WebEditorConfig::default();
            config.port = port;
            config.access_mode = if is_read_only { AccessMode::Read } else { AccessMode::ReadWrite };
            config.is_standalone = true;
            web_editor::launch_sync(config, WebEditorInput::from_path(&path));
            Ok(())
        }
        _ => {
            let config = main_input_command.apply(load_config(&config_path));
            App::new(config)?.run_until_completion(input_command)
        }
    }
}

fn run_init(config_path: &Path, input_command: InputCommand) -> Result<(), AppError> {
    if let InputCommand::Initialize { name, use_existing } = input_command {
        let repository_path = if !use_existing {
            base_dir().join(name)
        } else {
            Path::new(&name).to_owned()
        };

        let file_config = FileConfig::load(&config_path).unwrap_or_else(|_| FileConfig::new(&repository_path));

        if !use_existing {
            std::fs::create_dir_all(&repository_path)?;
            git2::Repository::init(&repository_path)?;
        } else {
            if !repository_path.exists() {
                return Err(AppError::Input("Repository does not exist.".to_owned()));
            }
        }

        file_config.save(&config_path)?;
    }

    Ok(())
}

fn load_config(config_path: &Path) -> Config {
    Config::load(&config_path).expect(&format!("Expected a valid config file at '{}', please run 'init' command to setup", config_path.to_str().unwrap()))
}