use std::path::{Path, PathBuf};

use comrak::nodes::NodeValue;

use structopt::StructOpt;

use thiserror::Error;

mod helpers;
mod model;
mod querying;
mod command;
mod markdown;
mod snippets;

use crate::command::{Command, CommandInterpreter, CommandInterpreterError};
use crate::model::{NoteMetadata, NoteMetadataStorage};
use crate::querying::{Finder, FindQuery, ListDirectory, ListTree, print_list_directory_results, print_note_metadata_results, QueryingError, RegexMatcher, StringMatcher};

fn main() {
    let repository = Path::new("test-notes");

    let input_command: InputCommand = InputCommand::from_args();
    if let Err(err) = run(input_command, repository) {
        println!("{}.", err.to_string());
        std::process::exit(1);
    }
}

fn run(input_command: InputCommand, repository: &Path) -> Result<(), RunError> {
    match input_command {
        InputCommand::AddFakeData => {
            CommandInterpreter::new(&repository)?.execute(vec![
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
            CommandInterpreter::new(&repository)?.execute(vec![
                Command::AddNote { path, tags},
                Command::Commit
            ])?;
        }
        InputCommand::Edit { path } => {
            CommandInterpreter::new(&repository)?.execute(vec![
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

            CommandInterpreter::new(&repository)?.execute(commands)?;
        }
        InputCommand::PrintContent { path, only_code } => {
            let content = NoteMetadataStorage::from_dir(repository)?.get_content(&path)?;

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
                ConsoleInputFinder::Tags { tags } => {
                    FindQuery::Tags(tags.into_iter().map(|tag| StringMatcher::new(&tag)).collect())
                }
                ConsoleInputFinder::Name { name } => {
                    FindQuery::Path(RegexMatcher::new(&name))
                }
                ConsoleInputFinder::Id { id } => {
                    FindQuery::Id(RegexMatcher::new(&id))
                }
                ConsoleInputFinder::Created { parts } => {
                    FindQuery::Created(parts)
                }
                ConsoleInputFinder::Updated { parts } => {
                    FindQuery::LastUpdated(parts)
                }
            };

            let finder = Finder::new(&repository)?;
            let results = finder.find(&query)?;
            print_note_metadata_results(&results);
        }
        InputCommand::ListDirectory { query } => {
            let notes_metadata = NoteMetadata::load_all_to_vec(repository)?;
            let list_directory = ListDirectory::new(&notes_metadata)?;

            let results = list_directory.list(query.as_ref().map(|x| x.as_str()));
            print_list_directory_results(&results)
        }
        InputCommand::Tree { prefix } => {
            let notes_metadata = NoteMetadata::load_all_to_vec(repository)?;
            let list_tree = ListTree::new(&notes_metadata)?;
            list_tree.list(prefix.as_ref().map(|x| x.as_path()));
        }
    }

    Ok(())
}

#[derive(Debug, StructOpt)]
enum InputCommand {
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
    Finder(ConsoleInputFinder),
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
enum ConsoleInputFinder {
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
pub enum RunError {
    #[error("{0}")]
    Command(CommandInterpreterError),

    #[error("{0}")]
    Querying(QueryingError),

    #[error("{0}")]
    IO(std::io::Error)
}

impl From<CommandInterpreterError> for RunError {
    fn from(err: CommandInterpreterError) -> Self {
        RunError::Command(err)
    }
}

impl From<QueryingError> for RunError {
    fn from(err: QueryingError) -> Self {
        RunError::Querying(err)
    }
}

impl From<std::io::Error> for RunError {
    fn from(err: std::io::Error) -> Self {
        RunError::IO(err)
    }
}