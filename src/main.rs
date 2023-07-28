use std::path::Path;

use rustyline::{DefaultEditor, Editor};
use structopt::{clap, StructOpt};

mod helpers;
mod model;
mod querying;
mod command;
mod markdown;
mod snippets;
mod app;

use crate::app::{AppError, Application, Config, InputCommand};

fn main() {
    let config = Config {
        repository: Path::new("test-notes").to_path_buf()
    };

    let input_command: InputCommand = InputCommand::from_args();
    if let Err(err) = run(config, input_command) {
        println!("{}.", err.to_string());
        std::process::exit(1);
    }
}

fn run(config: Config, input_command: InputCommand) -> Result<(), AppError> {
    if input_command.is_interactive() {
        run_interactive(config)
    } else {
        Application::new(config)?.run(input_command)
    }
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

fn input_command_interactive(line: &str) -> Result<InputCommand, clap::Error> {
    Ok(InputCommand::from_clap(
        &InputCommand::clap()
            .setting(clap::AppSettings::NoBinaryName)
            .get_matches_from_safe(line.split(" "))?
    ))
}