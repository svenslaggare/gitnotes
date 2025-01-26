use std::path::{Path, PathBuf};
use std::io::stdout;

use crossterm::cursor::{MoveDown, MoveUp, RestorePosition, SavePosition};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, read};
use crossterm::ExecutableCommand;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use fnv::FnvHashMap;
use structopt::{clap, StructOpt};

use rustyline::completion::{Completer, Pair};
use rustyline::{Context, Editor};
use rustyline::error::ReadlineError;
use rustyline_derive::{Helper, Highlighter, Hinter};
use rustyline::validate::{ValidationContext, ValidationResult, Validator};
use rustyline::history::FileHistory;

use substring::Substring;

use crate::app::{AppError, App, InputCommand, MainInputCommand};
use crate::config::config_path;
use crate::model::{NoteFileTree, NoteMetadata};

pub fn run(main_input_command: MainInputCommand) -> Result<(), AppError> {
    let mut app = App::new(main_input_command.apply(crate::load_config(&config_path())))?;
    let mut history = FileHistory::new();
    let mut notes_version = 0;

    loop {
        if !run_app(&mut app, &mut history, &mut notes_version)? {
            break;
        }
    }

    Ok(())
}

fn run_app(app: &mut App, history: &mut FileHistory, notes_version: &mut u64) -> Result<bool, AppError> {
    let notes_metadata = app.note_metadata_storage()?.notes().cloned().collect::<Vec<_>>();
    let note_file_tree = NoteFileTree::from_iter(notes_metadata.iter()).unwrap();

    let mut line_editor = Editor::new().unwrap();
    line_editor.set_helper(Some(AutoCompletion::new(note_file_tree)));

    if let Some(helper) = line_editor.helper_mut() {
        helper.update(app);
    }

    std::mem::swap(history, line_editor.history_mut());

    while let Ok(mut line) = line_editor.readline("> ") {
        if line.ends_with('\n') {
            line.pop();
        }

        line_editor.add_history_entry(line.clone()).unwrap();

        if let Some(helper) = line_editor.helper_mut() {
            helper.update(app);
        }

        match input_command_interactive(&line) {
            Ok(input_command) => {
                if let Err(err) = app.run_until_completion(input_command) {
                    println!("{}.", err);
                }
            }
            Err(err) => {
                print!("{}", err);
            }
        }

        if app.has_changed(notes_version) {
            std::mem::swap(history, line_editor.history_mut());
            return Ok(true);
        }
    }

    Ok(false)
}

pub fn select<F: Fn(&str, usize) -> String>(
    command_name: &str,
    num_matches: usize,
    create_input_line: F
) -> Result<Option<InputCommand>, AppError> {
    if num_matches == 0 {
        return Ok(None);
    }

    let mut current_index: Option<usize> = None;
    stdout().execute(SavePosition)?;
    enable_raw_mode()?;

    loop {
        let event = read()?;
        match event {
            Event::Key(KeyEvent { code: KeyCode::Up, .. }) => {
                match current_index.as_mut() {
                    Some(current_index) if *current_index > 0 => {
                        stdout().execute(MoveUp(1))?;
                        *current_index -= 1;
                    }
                    None => {
                        stdout().execute(MoveUp(1))?;
                        current_index = Some(num_matches - 1);
                    }
                    _ => {}
                }
            }
            Event::Key(KeyEvent { code: KeyCode::Down, .. }) => {
                match current_index.as_mut() {
                    Some(current_index) if *current_index < num_matches - 1 => {
                        stdout().execute(MoveDown(1))?;
                        *current_index += 1;
                    }
                    _ => {}
                }
            }
            Event::Key(KeyEvent { code: KeyCode::Char('c'), modifiers: KeyModifiers::CONTROL, .. }) => {
                current_index = None;
                break;
            }
            Event::Key(KeyEvent { code: KeyCode::Enter, .. }) => {
                break;
            }
            _ => {}
        }
    }
    stdout().execute(RestorePosition)?;
    disable_raw_mode()?;

    if let Some(current_index) = current_index {
        input_command_interactive(&create_input_line(command_name, current_index))
            .map(|command| Some(command))
            .map_err(|err| AppError::Input(err))
    } else {
        Ok(None)
    }
}

pub fn select_with_note_metadata(command_name: &str, notes_metadata: &Vec<&NoteMetadata>) -> Result<Option<InputCommand>, AppError> {
    select(
        command_name,
        notes_metadata.len(),
        |command_name, index: usize| {
            format!("{} {}", command_name, notes_metadata[index].path.to_str().unwrap())
        }
    )
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

pub enum AutoCompletionCommand {
    Regular {
        name: String
    },
    Path {
        name: String
    },
    SubCommand {
        name: String,
        sub_commands: Vec<String>
    }
}

impl AutoCompletionCommand {
    pub fn name(&self) -> &str {
        match self {
            AutoCompletionCommand::Regular { name, .. } => name,
            AutoCompletionCommand::Path { name, .. } => name,
            AutoCompletionCommand::SubCommand { name, .. } => name
        }
    }
}

#[derive(Helper, Highlighter, Hinter)]
struct AutoCompletion<'a> {
    commands: FnvHashMap<String, AutoCompletionCommand>,
    note_file_tree: NoteFileTree<'a>,
    working_dir: Option<PathBuf>
}

impl<'a> AutoCompletion<'a> {
    pub fn new(note_file_tree: NoteFileTree<'a>) -> AutoCompletion<'a> {
        let commands = vec![
            AutoCompletionCommand::Path { name: "add".to_owned() },
            AutoCompletionCommand::Path { name: "rm".to_owned() },
            AutoCompletionCommand::Path { name: "edit".to_owned() },
            AutoCompletionCommand::Path { name: "mv".to_owned() },
            AutoCompletionCommand::Path { name: "cat".to_owned() },
            AutoCompletionCommand::Path { name: "show".to_owned() },
            AutoCompletionCommand::Path { name: "convert".to_owned() },
            AutoCompletionCommand::Path { name: "info".to_owned() },
            AutoCompletionCommand::Path { name: "tree".to_owned() },
            AutoCompletionCommand::Path { name: "cd".to_owned() },
            AutoCompletionCommand::Regular { name: "begin".to_owned() },
            AutoCompletionCommand::Regular { name: "commit".to_owned() },
            AutoCompletionCommand::Regular { name: "config".to_owned() },
            AutoCompletionCommand::SubCommand {
                name: "find".to_owned(),
                sub_commands: vec![
                    "tag".to_string(),
                    "name".to_owned(),
                    "id".to_owned(),
                    "created".to_owned(),
                    "updated".to_owned()
                ]
            },
            AutoCompletionCommand::Regular { name: "grep".to_owned() },
            AutoCompletionCommand::Regular { name: "help".to_owned() },
            AutoCompletionCommand::Regular { name: "log".to_owned() },
            AutoCompletionCommand::Regular { name: "switch".to_owned() },
            AutoCompletionCommand::Regular { name: "undo".to_owned() },
            AutoCompletionCommand::Regular { name: "pwd".to_owned() },
            AutoCompletionCommand::SubCommand {
                name: "remote".to_owned(),
                sub_commands: vec!["list".to_owned(), "add".to_owned(), "remove".to_owned()]
            },
            AutoCompletionCommand::Regular { name: "sync".to_owned() },
            AutoCompletionCommand::Regular { name: "update-symbolic-links".to_owned() },
            AutoCompletionCommand::Regular { name: "open-notes".to_owned() },
        ];

        AutoCompletion {
            commands: FnvHashMap::from_iter(commands.into_iter().map(|command| (command.name().to_owned(), command))),
            note_file_tree,
            working_dir: None
        }
    }

    pub fn update(&mut self, app: &mut App) {
        self.working_dir = app.working_dir().ok();
    }

    fn current_command<'b>(&'b self, line: &'b str) -> Option<&'b str> {
        for (index, current) in line.chars().enumerate() {
            if current.is_whitespace() {
                return Some(line.substring(0, index));
            }
        }

        None
    }

    fn get_note_tree(&self, current_word: &str, path_segment_done: bool) -> Option<&'a NoteFileTree> {
        if path_segment_done {
            let path = Path::new(&current_word);
            let path = if current_word.ends_with("/") {
                path
            } else {
                path.parent().unwrap_or(path)
            };

            if path.is_absolute() {
                let path = path.strip_prefix("/").ok()?;
                self.note_file_tree.find(path)
            } else {
                self.get_base_note_tree().find(&path)
            }
        } else {
            Some(self.get_base_note_tree())
        }
    }

    fn get_base_note_tree(&self) -> &'a NoteFileTree {
        if let Some(working_dir) = self.working_dir.as_ref() {
            self.note_file_tree.find(&working_dir).unwrap_or_else(|| &self.note_file_tree)
        } else {
            &self.note_file_tree
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
        let mut num_done_path_segments = 0;
        for char in line.chars().rev() {
            if char.is_whitespace() {
                break;
            }

            if char == '/' {
                path_segment_done = true;
                num_done_path_segments += 1;
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

        if let Some(first) = current_word.chars().next() {
            if first == '/' && num_done_path_segments == 1 {
                path_segment_done = false;
            }
        }

        let mut current_completion = &current_word;
        let mut current_completion_length = current_word_length;

        let iterator: Box<dyn Iterator<Item=(&str, bool)>> = match self.current_command(line) {
            None => {
                Box::new(self.commands.values().map(|command| (command.name(), false)))
            }
            Some(command) => {
                if let Some(command) = self.commands.get(command) {
                    match command {
                        AutoCompletionCommand::Path { .. } => {
                            current_completion = &current_path_segment;
                            current_completion_length = current_path_segment_length;

                            self.get_note_tree(&current_word, path_segment_done)
                                .map(|note_file_tree| {
                                    note_file_tree.children().map(|children| {
                                        let iter: Box<dyn Iterator<Item=(&str, bool)>> = Box::new(
                                            children
                                                .iter()
                                                .map(|(name, tree)| (name.to_str().unwrap(), !tree.is_leaf()))
                                        );
                                        iter
                                    })
                                })
                                .flatten()
                                .unwrap_or_else(|| Box::new(std::iter::empty()))
                        }
                        AutoCompletionCommand::SubCommand { sub_commands, .. } => {
                            Box::new(sub_commands.iter().map(|command| (command.as_str(), false)))
                        }
                        _ => Box::new(std::iter::empty())
                    }
                } else {
                    Box::new(std::iter::empty())
                }
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
