use std::path::Path;
use home::home_dir;

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
mod app;

use crate::app::{AppError, Application, InputCommand};
use crate::config::{Config, FileConfig};

fn main() {
    let home = home_dir().expect("Unable to determine home folder.");
    let base_dir = home.join(".gitnotes");

    let input_command: InputCommand = InputCommand::from_args();
    if let Err(err) = run(&base_dir, input_command) {
        println!("{}.", err.to_string());
        std::process::exit(1);
    }
}

fn run(base_dir: &Path, input_command: InputCommand) -> Result<(), AppError> {
    let config_path = base_dir.join("config.toml");
    let load_config = || {
        Config::load(&config_path).expect(&format!("Expected a valid config file at '{}'", config_path.to_str().unwrap()))
    };

    match input_command {
        InputCommand::Interactive => {
            run_interactive(load_config())
        }
        InputCommand::Initialize { .. } => {
            run_init(base_dir, &config_path, input_command)
        }
        _ => {
            Application::new(load_config())?.run(input_command)
        }
    }
}

fn run_init(base_dir: &Path, config_path: &Path, input_command: InputCommand) -> Result<(), AppError> {
    if let InputCommand::Initialize { name } = input_command {
        let repository_path = base_dir.join(name);

        let file_config = FileConfig::load(&config_path).unwrap_or_else(|_| FileConfig::new(&repository_path));

        std::fs::create_dir_all(&repository_path)?;
        git2::Repository::init(&repository_path)?;

        file_config.save(&config_path)?;
    }

    Ok(())
}

fn run_interactive(config: Config) -> Result<(), AppError> {
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

fn input_command_interactive(line: &str) -> Result<InputCommand, String> {
    let words = shellwords::split(line).map_err(|err| err.to_string())?;
    Ok(InputCommand::from_clap(
        &InputCommand::clap()
            .setting(clap::AppSettings::NoBinaryName)
            .get_matches_from_safe(words).map_err(|err| err.to_string())?
    ))
}