use std::path::{Path, PathBuf};

use serde::Deserialize;
use regex::Regex;
use thiserror::Error;

use structopt::StructOpt;

use comrak::nodes::NodeValue;
use tempfile::TempDir;

use crate::command::{Command, CommandInterpreter, CommandInterpreterError};
use crate::markdown;
use crate::model::{NoteMetadataStorage};
use crate::querying::{Finder, FindQuery, GitLog, ListDirectory, ListTree, print_list_directory_results, print_note_metadata_results, QueryingError, RegexMatcher, Searcher, StringMatcher};

#[derive(Debug, Deserialize)]
pub struct Config {
    pub repository: PathBuf
}

pub struct Application {
    config: Config,
    command_interpreter: CommandInterpreter,
    note_metadata_storage: Option<NoteMetadataStorage>
}

impl Application {
    pub fn new(config: Config) -> Result<Application, AppError> {
        let command_interpreter = CommandInterpreter::new(&config.repository)?;
        Ok(
            Application {
                config,
                command_interpreter,
                note_metadata_storage: None
            }
        )
    }

    pub fn run(&mut self, input_command: InputCommand) -> Result<(), AppError> {
        match input_command {
            InputCommand::Interactive => {}
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
                self.clear_cache();
            }
            InputCommand::Add { path, tags } => {
                self.command_interpreter.execute(vec![
                    Command::AddNote { path, tags },
                    Command::Commit
                ])?;
                self.clear_cache();
            }
            InputCommand::Edit { path, clear_tags, add_tags } => {
                self.command_interpreter.execute(vec![
                    Command::EditNoteContent { path, clear_tags, add_tags },
                    Command::Commit
                ])?;
                self.clear_cache();
            }
            InputCommand::Move { source, destination } => {
                self.command_interpreter.execute(vec![
                    Command::MoveNote { source, destination },
                    Command::Commit
                ])?;
                self.clear_cache();
            }
            InputCommand::Remove { path } => {
                self.command_interpreter.execute(vec![
                    Command::RemoveNote { path },
                    Command::Commit
                ])?;
                self.clear_cache();
            }
            InputCommand::RunSnippet { path, save_output } => {
                let mut commands = vec![
                    Command::RunSnippet { path, save_output }
                ];

                if save_output {
                    commands.push(Command::Commit);
                }

                self.command_interpreter.execute(commands)?;
                self.clear_cache();
            }
            InputCommand::PrintContent { path, only_code, only_output } => {
                let content = self.note_metadata_storage()?.get_content(&path)?;

                if only_code || only_output {
                    let arena = markdown::storage();
                    let root = markdown::parse(&arena, &content);

                    markdown::visit_code_blocks::<CommandInterpreterError, _>(
                        &root,
                        |current_node| {
                            if let NodeValue::CodeBlock(ref block) = current_node.data.borrow().value {
                                println!("{}", block.literal);
                            }

                            Ok(())
                        },
                        only_code,
                        only_output
                    )?;
                } else {
                    println!("{}", content);
                }
            }
            InputCommand::ListDirectory { query } => {
                let list_directory = ListDirectory::new(self.note_metadata_storage()?)?;
                let results = list_directory.list(query.as_ref().map(|x| x.as_str()));
                print_list_directory_results(&results)?
            }
            InputCommand::Tree { prefix } => {
                let list_tree = ListTree::new(self.note_metadata_storage()?)?;
                list_tree.list(prefix.as_ref().map(|x| x.as_path()));
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

                let finder = Finder::new(self.note_metadata_storage()?)?;
                let results = finder.find(&query)?;
                print_note_metadata_results(&results);
            }
            InputCommand::Search { mut query, case_sensitive } => {
                if !case_sensitive {
                    query = "(?i)".to_owned() + &query;
                }
                let query = Regex::new(&query)?;

                let searcher = Searcher::new(self.note_metadata_storage()?)?;
                searcher.search(&query)?;
            }
            InputCommand::Log { count } => {
                let git_log = GitLog::new(&self.config.repository, count)?;
                git_log.print()?;
            }
        }

        Ok(())
    }

    fn clear_cache(&mut self) {
        self.note_metadata_storage = None;
    }

    fn note_metadata_storage(&mut self) -> std::io::Result<&NoteMetadataStorage> {
        if self.note_metadata_storage.is_none() {
            let note_metadata_storage = NoteMetadataStorage::from_dir(&self.config.repository)?;
            self.note_metadata_storage = Some(note_metadata_storage);
        }

        Ok(self.note_metadata_storage.as_ref().unwrap())
    }
}

#[derive(Debug, StructOpt)]
#[structopt(about="CLI & Git based notes/snippet application")]
#[structopt(global_setting=structopt::clap::AppSettings::AllowNegativeNumbers)]
pub enum InputCommand {
    /// Runs in interactive mode
    Interactive,
    /// Adds fake data.
    AddFakeData,
    /// Creates a new note.
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
        path: PathBuf,
        /// Clears the tags.
        #[structopt(long)]
        clear_tags: bool,
        /// Adds tags. These are added after tags are cleared.
        #[structopt(long)]
        add_tags: Vec<String>
    },
    /// Moves a note to a new location
    #[structopt(name="mv")]
    Move {
        /// The absolute path of the note. Id also work.
        source: PathBuf,
        /// The absolute path of the new destination.
        destination: PathBuf,
    },
    /// Removes a note
    #[structopt(name="rm")]
    Remove {
        /// The absolute path of the note. Id also work.
        path: PathBuf
    },
    /// Runs the code snippet contained in a note.
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
        /// Print only code content.
        #[structopt(long="code")]
        only_code: bool,
        /// Print only output content.
        #[structopt(long="output")]
        only_output: bool
    },
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
    },
    /// Searches for note based on properties.
    #[structopt(name="find")]
    Finder(InputCommandFinder),
    /// Searches for note based on content.
    #[structopt(name="grep")]
    Search {
        /// The regex query.
        query: String,
        /// Indicates if the match is cans sensitive
        #[structopt(long="no-ignore-case")]
        case_sensitive: bool
    },
    /// Lists git commits
    Log {
        /// The number of commits to show. -1 for all.
        #[structopt(default_value="5")]
        count: isize
    }
}

impl InputCommand {
    pub fn is_interactive(&self) -> bool {
        if let InputCommand::Interactive = self {
            true
        } else {
            false
        }
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
    Regex(regex::Error),

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

impl From<regex::Error> for AppError {
    fn from(err: regex::Error) -> Self {
        AppError::Regex(err)
    }
}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::IO(err)
    }
}

#[test]
fn test_add_and_run_snippet() {
    let temp_repository_dir = TempDir::new().unwrap();
    let config = Config {
        repository: temp_repository_dir.path().to_path_buf()
    };
    let repository = git2::Repository::init(&config.repository).unwrap();

    let note_path = Path::new("2023/07/sample.py");
    let note_content = r#"Hello, World!

``` python
import numpy as np
print(np.square(np.arange(0, 10)))
```
"#.to_string();

    let note_content_output = r#"Hello, World\!

``` python
import numpy as np
print(np.square(np.arange(0, 10)))
```

``` output
[ 0  1  4  9 16 25 36 49 64 81]
```
"#.to_string();

    let note_content2 = r#"Hello, World!

``` python
import numpy as np
print(np.square(np.arange(0, 11)))
```

``` output
[ 0  1  4  9 16 25 36 49 64 81]
```
"#.to_string();

    let note_content_output2 = r#"Hello, World\!

``` python
import numpy as np
print(np.square(np.arange(0, 11)))
```

``` output
[  0   1   4   9  16  25  36  49  64  81 100]
```
"#.to_string();

    let mut app = Application::new(config).unwrap();

    app.command_interpreter.execute(vec![
        Command::AddNoteWithContent {
            path: note_path.to_path_buf(),
            tags: vec!["python".to_owned()],
            content: note_content.clone()
        },
        Command::Commit
    ]).unwrap();
    assert_eq!(note_content, app.note_metadata_storage().unwrap().get_content(note_path).unwrap());
    assert_eq!(1, repository.reflog("HEAD").unwrap().len());

    app.run(InputCommand::RunSnippet { path: note_path.to_owned(), save_output: true }).unwrap();
    assert_eq!(note_content_output, app.note_metadata_storage().unwrap().get_content(note_path).unwrap());
    assert_eq!(2, repository.reflog("HEAD").unwrap().len());

    app.command_interpreter.execute(vec![
        Command::EditNoteSetContent {
            path: note_path.to_path_buf(),
            content: note_content2.clone()
        },
        Command::Commit
    ]).unwrap();
    assert_eq!(note_content2, app.note_metadata_storage().unwrap().get_content(note_path).unwrap());
    assert_eq!(3, repository.reflog("HEAD").unwrap().len());

    app.run(InputCommand::RunSnippet { path: note_path.to_owned(), save_output: true }).unwrap();
    assert_eq!(note_content_output2, app.note_metadata_storage().unwrap().get_content(note_path).unwrap());
    assert_eq!(4, repository.reflog("HEAD").unwrap().len());
}

#[test]
fn test_add_and_move() {
    let temp_repository_dir = TempDir::new().unwrap();
    let config = Config {
        repository: temp_repository_dir.path().to_path_buf()
    };
    let repository = git2::Repository::init(&config.repository).unwrap();

    let note_path = Path::new("2023/07/sample.py");
    let note_path2 = Path::new("2023/07/01/sample.py");
    let note_content = r#"Hello, World!

``` python
import numpy as np
print(np.square(np.arange(0, 10)))
```
"#.to_string();

    let mut app = Application::new(config).unwrap();

    app.command_interpreter.execute(vec![
        Command::AddNoteWithContent {
            path: note_path.to_path_buf(),
            tags: vec!["python".to_owned()],
            content: note_content.clone()
        },
        Command::Commit
    ]).unwrap();
    assert_eq!(note_content, app.note_metadata_storage().unwrap().get_content(note_path).unwrap());
    assert_eq!(1, repository.reflog("HEAD").unwrap().len());

    app.run(InputCommand::Move { source: note_path.to_owned(), destination: note_path2.to_owned() }).unwrap();
    assert_eq!(false, app.note_metadata_storage().unwrap().get_content(note_path).is_ok());
    assert_eq!(note_content, app.note_metadata_storage().unwrap().get_content(note_path2).unwrap());
    assert_eq!(2, repository.reflog("HEAD").unwrap().len());
}

#[test]
fn test_add_and_remove() {
    let temp_repository_dir = TempDir::new().unwrap();
    let config = Config {
        repository: temp_repository_dir.path().to_path_buf()
    };
    let repository = git2::Repository::init(&config.repository).unwrap();

    let note_path = Path::new("2023/07/sample.py");
    let note_content = r#"Hello, World!

``` python
import numpy as np
print(np.square(np.arange(0, 10)))
```
"#.to_string();

    let mut app = Application::new(config).unwrap();

    app.command_interpreter.execute(vec![
        Command::AddNoteWithContent {
            path: note_path.to_path_buf(),
            tags: vec!["python".to_owned()],
            content: note_content.clone()
        },
        Command::Commit
    ]).unwrap();
    assert_eq!(note_content, app.note_metadata_storage().unwrap().get_content(note_path).unwrap());
    assert_eq!(1, repository.reflog("HEAD").unwrap().len());

    app.run(InputCommand::Remove { path: note_path.to_owned() }).unwrap();
    assert_eq!(false, app.note_metadata_storage().unwrap().get(note_path).is_some());
    assert_eq!(false, app.note_metadata_storage().unwrap().get_content(note_path).is_ok());
    assert_eq!(2, repository.reflog("HEAD").unwrap().len());
}