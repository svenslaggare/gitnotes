use std::path::Path;
use structopt::StructOpt;

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
    Application::new(config)?.run(input_command)
}