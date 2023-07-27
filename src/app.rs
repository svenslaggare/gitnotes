use std::path::{Path, PathBuf};

use comrak::nodes::NodeValue;

use structopt::StructOpt;

use thiserror::Error;

use crate::command::{Command, CommandInterpreter, CommandInterpreterError};
use crate::markdown;
use crate::model::{NoteMetadata, NoteMetadataStorage};
use crate::querying::{Finder, FindQuery, ListDirectory, ListTree, print_list_directory_results, print_note_metadata_results, QueryingError, RegexMatcher, StringMatcher};

pub struct Application {
    repository: PathBuf,
    command_interpreter: CommandInterpreter
}

impl Application {
    pub fn new(repository: &Path) -> Result<Application, AppError> {
        Ok(
            Application {
                repository: repository.to_path_buf(),
                command_interpreter: CommandInterpreter::new(repository)?
            }
        )
    }

    pub fn run(&mut self, input_command: InputCommand) -> Result<(), AppError> {
        match input_command {
            InputCommand::AddFakeData => {
                self.command_interpreter.execute(vec![
                    Command::AddNoteWithContent {
                        path: Path::new("2022/05/test1").to_owned(),
                        tags: vec!["x".to_owned(), "y".to_owned()],
                        content: r#"Hello, World!

``` python
import numpy as np
print(np.square(np.arange(0, 10)))
```
"#.to_string()
                    },
                    Command::AddNoteWithContent {
                        path: Path::new("2023/05/test2").to_owned(),
                        tags: vec!["x".to_owned(), "z".to_owned()],
                        content: "Hello, Stupid World!".to_string()
                    },
                    Command::AddNoteWithContent {
                        path: Path::new("2023/test3").to_owned(),
                        tags: vec!["x".to_owned(), "y".to_owned()],
                        content: "Hello, New World!".to_string()
                    },
                    Command::Commit
                ])?;
            }
            InputCommand::Add { path, tags } => {
                self.command_interpreter.execute(vec![
                    Command::AddNote { path, tags },
                    Command::Commit
                ])?;
            }
            InputCommand::Edit { path } => {
                self.command_interpreter.execute(vec![
                    Command::EditNoteContent { path },
                    Command::Commit
                ])?;
            }
            InputCommand::RunSnippet { path, save_output } => {
                let mut commands = vec![
                    Command::RunSnippet { path, save_output }
                ];

                if save_output {
                    commands.push(Command::Commit);
                }

                self.command_interpreter.execute(commands)?;
            }
            InputCommand::PrintContent { path, only_code } => {
                let content = NoteMetadataStorage::from_dir(&self.repository)?.get_content(&path)?;

                if only_code {
                    let arena = markdown::storage();
                    let root = markdown::parse(&arena, &content);

                    markdown::visit_code_blocks::<CommandInterpreterError, _>(
                        &root,
                        |current_node| {
                            if let NodeValue::CodeBlock(ref block) = current_node.data.borrow().value {
                                println!("{}", block.literal);
                            }

                            Ok(())
                        }
                    )?;
                } else {
                    println!("{}", content);
                }
            }
            InputCommand::Finder(finder) => {
                let query = match finder {
                    InputCommandFinder::Tags { tags } => {
                        FindQuery::Tags(tags.into_iter().map(|tag| StringMatcher::new(&tag)).collect())
                    }
                    InputCommandFinder::Name { name } => {
                        FindQuery::Path(RegexMatcher::new(&name))
                    }
                    InputCommandFinder::Id { id } => {
                        FindQuery::Id(RegexMatcher::new(&id))
                    }
                    InputCommandFinder::Created { parts } => {
                        FindQuery::Created(parts)
                    }
                    InputCommandFinder::Updated { parts } => {
                        FindQuery::LastUpdated(parts)
                    }
                };

                let finder = Finder::new(&self.repository)?;
                let results = finder.find(&query)?;
                print_note_metadata_results(&results);
            }
            InputCommand::ListDirectory { query } => {
                let notes_metadata = NoteMetadata::load_all_to_vec(&self.repository)?;
                let list_directory = ListDirectory::new(&notes_metadata)?;

                let results = list_directory.list(query.as_ref().map(|x| x.as_str()));
                print_list_directory_results(&results)
            }
            InputCommand::Tree { prefix } => {
                let notes_metadata = NoteMetadata::load_all_to_vec(&self.repository)?;
                let list_tree = ListTree::new(&notes_metadata)?;
                list_tree.list(prefix.as_ref().map(|x| x.as_path()));
            }
        }

        Ok(())
    }
}

#[derive(Debug, StructOpt)]
pub enum InputCommand {
    /// Adds fake data.
    AddFakeData,
    /// Adds a new note.
    Add {
        /// The path of the note.
        path: PathBuf,
        /// The tags of the note.
        #[structopt(long)]
        tags: Vec<String>
    },
    /// Edits an existing note.
    Edit {
        /// The absolute path of the note. Id also work.
        path: PathBuf
    },
    /// Runs the code snipper contained in a note.
    #[structopt(name="run")]
    RunSnippet {
        /// The absolute path of the note. Id also work.
        path: PathBuf,
        /// Saves the output of the snippet inside the note.
        #[structopt(long="save")]
        save_output: bool
    },
    /// Prints the content of a note.
    #[structopt(name="cat")]
    PrintContent {
        /// The absolute path of the note. Id also work.
        path: PathBuf,
        /// Print only code.
        #[structopt(long="code")]
        only_code: bool
    },
    /// Searches for note based on properties.
    #[structopt(name="find")]
    Finder(InputCommandFinder),
    /// Lists note in a directory.
    #[structopt(name="ls")]
    ListDirectory {
        /// The directory to list.
        query: Option<String>
    },
    /// Lists note tree structure.
    Tree {
        /// List tree starting at the given prefix.
        prefix: Option<PathBuf>
    }
}

#[derive(Debug, StructOpt)]
pub enum InputCommandFinder {
    /// Searches based on tags.
    Tags {
        /// The tags that the note must contain (AND).
        tags: Vec<String>
    },
    /// Searches based on name.
    Name {
        /// Regex pattern.
        name: String
    },
    /// Searches based on id.
    Id {
        /// Regex pattern
        id: String
    },
    /// Searches based on created date
    Created {
        /// First element is year, then month, etc. All parts are optional.
        parts: Vec<i32>
    },
    /// Searches based on updated date
    Updated {
        /// First element is year, then month, etc. All parts are optional.
        parts: Vec<i32>
    }
}

#[derive(Error, Debug)]
pub enum AppError {
    #[error("{0}")]
    Command(CommandInterpreterError),

    #[error("{0}")]
    Querying(QueryingError),

    #[error("{0}")]
    IO(std::io::Error)
}

impl From<CommandInterpreterError> for AppError {
    fn from(err: CommandInterpreterError) -> Self {
        AppError::Command(err)
    }
}

impl From<QueryingError> for AppError {
    fn from(err: QueryingError) -> Self {
        AppError::Querying(err)
    }
}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::IO(err)
    }
}