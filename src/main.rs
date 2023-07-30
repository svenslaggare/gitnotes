use std::path::{Path};

use rustyline::{DefaultEditor};
use structopt::{clap, StructOpt};

mod config;
mod helpers;
mod model;
mod querying;
mod command;
mod markdown;
mod snippets;
mod editor;
mod tags;
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
        if let Err(err) = run_interactive() {
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
        _ => {
            Application::new(load_config(&config_path))?.run(input_command)
        }
    }
}

fn run_init(config_path: &Path, input_command: InputCommand) -> Result<(), AppError> {
    if let InputCommand::Initialize { name } = input_command {
        let repository_path = base_dir().join(name);

        let file_config = FileConfig::load(&config_path).unwrap_or_else(|_| FileConfig::new(&repository_path));

        std::fs::create_dir_all(&repository_path)?;
        git2::Repository::init(&repository_path)?;

        file_config.save(&config_path)?;
    }

    Ok(())
}

fn run_interactive() -> Result<(), AppError> {
    let config = load_config(&config_path());
    let mut app = Application::new(config)?;

    let mut line_editor = DefaultEditor::new().unwrap();
    while let Ok(mut line) = line_editor.readline("> ") {
        if line.ends_with('\n') {
            line.pop();
        }

        line_editor.add_history_entry(line.clone()).unwrap();

        match input_command_interactive(&line) {
            Ok(input_command) => {
                if let Err(err) = app.run(input_command) {
                    println!("{}.", err);
                }
            }
            Err(err) => {
                println!("{}", err);
            }
        }
    }

    Ok(())
}

fn load_config(config_path: &Path) -> Config {
    Config::load(&config_path).expect(&format!("Expected a valid config file at '{}', please run 'init' command to setup", config_path.to_str().unwrap()))
}

fn input_command_interactive(line: &str) -> Result<InputCommand, String> {
    let words = shellwords::split(line).map_err(|err| err.to_string())?;
    Ok(InputCommand::from_clap(
        &InputCommand::clap()
            .setting(clap::AppSettings::NoBinaryName)
            .get_matches_from_safe(words).map_err(|err| err.to_string())?
    ))
}