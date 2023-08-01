use std::path::Path;

use structopt::StructOpt;

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

use crate::app::{AppError, Application, InputCommand, MainInputCommand};
use crate::config::{Config, config_path, FileConfig};
use crate::helpers::base_dir;

fn main() {
    let main_input_command = MainInputCommand::from_args();

    if let Some(input_command) = main_input_command.command {
        if let Err(err) = run(input_command) {
            println!("{}.", err.to_string());
            std::process::exit(1);
        }
    } else  {
        if let Err(err) = interactive::run() {
            println!("{}.", err.to_string());
            std::process::exit(1);
        }
    }
}

fn run(input_command: InputCommand) -> Result<(), AppError> {
    let config_path = config_path();
    match input_command {
        InputCommand::Initialize { .. } => {
            run_init(&config_path, input_command)
        }
        InputCommand::WebEditor { path, port } => {
            web_editor::launch_sync(port, &path);
            Ok(())
        }
        _ => {
            Application::new(load_config(&config_path))?.run(input_command)
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