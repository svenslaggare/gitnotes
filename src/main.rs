use std::path::Path;
use structopt::StructOpt;

mod helpers;
mod model;
mod querying;
mod command;
mod markdown;
mod snippets;
mod app;

use crate::app::{AppError, Application, InputCommand};

fn main() {
    let repository = Path::new("test-notes");

    let input_command: InputCommand = InputCommand::from_args();
    if let Err(err) = run(repository, input_command) {
        println!("{}.", err.to_string());
        std::process::exit(1);
    }
}

fn run(repository: &Path, input_command: InputCommand) -> Result<(), AppError> {
    Application::new(repository)?.run(input_command)
}