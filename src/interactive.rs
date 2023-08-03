use std::path::{Path};
use std::collections::HashSet;

use structopt::{clap, StructOpt};

use rustyline::completion::{Completer, Pair};
use rustyline::{Context, Editor};
use rustyline::error::ReadlineError;
use rustyline_derive::{Helper, Highlighter, Hinter};
use rustyline::validate::{ValidationContext, ValidationResult, Validator};

use substring::Substring;

use crate::app::{AppError, Application, InputCommand};
use crate::config::config_path;
use crate::model::NoteFileTree;

pub fn run() -> Result<(), AppError> {
    let config = crate::load_config(&config_path());
    let mut app = Application::new(config)?;

    let notes_metadata = app.note_metadata_storage()?.notes().cloned().collect::<Vec<_>>();
    let note_file_tree = NoteFileTree::from_iter(notes_metadata.iter()).unwrap();

    let mut line_editor = Editor::new().unwrap();
    line_editor.set_helper(Some(AutoCompletion::new(note_file_tree)));

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
                print!("{}", err);
            }
        }
    }

    Ok(())
}

fn input_command_interactive(line: &str) -> Result<InputCommand, String> {
    let words = shellwords::split(line).map_err(|err| err.to_string())?;
    Ok(
        InputCommand::from_clap(
            &InputCommand::clap()
                .setting(clap::AppSettings::NoBinaryName)
                .get_matches_from_safe(words).map_err(|err| err.to_string())?
        )
    )
}

#[derive(Helper, Highlighter, Hinter)]
struct AutoCompletion<'a> {
    subcommands: Vec<String>,
    path_subcommands: HashSet<String>,
    note_file_tree: NoteFileTree<'a>
}

impl<'a> AutoCompletion<'a> {
    pub fn new(note_file_tree: NoteFileTree<'a>) -> AutoCompletion<'a> {
        AutoCompletion {
            subcommands: vec![
                "add".to_owned(),
                "begin".to_owned(),
                "cat".to_owned(),
                "commit".to_owned(),
                "config".to_owned(),
                "edit".to_owned(),
                "find".to_owned(),
                "grep".to_owned(),
                "help".to_owned(),
                "info".to_owned(),
                "log".to_owned(),
                "ls".to_owned(),
                "mv".to_owned(),
                "rm".to_owned(),
                "run".to_owned(),
                "show".to_owned(),
                "switch".to_owned(),
                "tree".to_owned()
            ],
            path_subcommands: HashSet::from_iter(vec![
               "add".to_owned(),
               "edit".to_owned(),
               "mv".to_owned(),
               "rm".to_owned(),
               "cat".to_owned(),
               "show".to_owned(),
               "ls".to_owned(),
               "tree".to_owned(),
               "info".to_owned(),
            ]),
            note_file_tree
        }
    }
}

impl<'a> Validator for AutoCompletion<'a> {
    fn validate(&self, _ctx: &mut ValidationContext) -> Result<ValidationResult, ReadlineError> {
        Ok(ValidationResult::Valid(None))
    }
}

impl<'a> Completer for AutoCompletion<'a> {
    type Candidate = Pair;

    fn complete(&self, line: &str, pos: usize, _ctx: &Context<'_>) -> Result<(usize, Vec<Pair>), ReadlineError> {
        let mut results = Vec::new();

        let mut current_word = Vec::new();
        let mut current_path_segment = Vec::new();
        let mut path_segment_done = false;
        for char in line.chars().rev() {
            if char.is_whitespace() {
                break;
            }

            if char == '/' {
                path_segment_done = true;
            }

            current_word.push(char);
            if !path_segment_done {
                current_path_segment.push(char);
            }
        }

        let current_word_length = current_word.len();
        let current_word = String::from_iter(current_word.into_iter().rev());

        let current_path_segment_length = current_path_segment.len();
        let current_path_segment = String::from_iter(current_path_segment.into_iter().rev());

        let mut current_completion = &current_word;
        let mut current_completion_length = current_word_length;

        let iterator:  Box<dyn Iterator<Item=(&str, bool)>> = match self.current_command(line) {
            None => {
                Box::new(self.subcommands.iter().map(|word| (word.as_str(), false)))
            }
            Some(command) if self.path_subcommands.contains(command) => {
                current_completion = &current_path_segment;
                current_completion_length = current_path_segment_length;

                let note_file_tree = if path_segment_done {
                    let path = Path::new(&current_word);
                    let path = if current_word.ends_with("/") {
                        path
                    } else {
                        path.parent().unwrap_or(path)
                    };

                    self.note_file_tree.find(&path)
                } else {
                    Some(&self.note_file_tree)
                };

                note_file_tree.map(|note_file_tree| {
                    note_file_tree.children().map(|children| {
                        let iter:  Box<dyn Iterator<Item=(&str, bool)>> = Box::new(children.iter().map(|(name, tree)| (name.to_str().unwrap(), !tree.is_leaf())));
                        iter
                    })
                }).flatten().unwrap_or_else(|| Box::new(std::iter::empty()))
            }
            _ => {
                Box::new(std::iter::empty())
            }
        };

        for (completion, is_dir) in iterator {
            if completion.starts_with(current_completion) {
                let mut completion = completion.to_owned();
                if is_dir {
                    completion.push('/');
                }

                results.push(Pair { display: completion.clone(), replacement: completion });
            }
        }

        Ok((pos - current_completion_length, results))
    }
}

impl<'a> AutoCompletion<'a> {
    fn current_command<'b>(&'b self, line: &'b str) -> Option<&'b str> {
        for (index, current) in line.chars().enumerate() {
            if current.is_whitespace() {
                return Some(line.substring(0, index));
            }
        }

        None
    }
}
