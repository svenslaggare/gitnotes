use std::cell::RefCell;
use std::io::{Read, stdin};
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use regex::Regex;
use thiserror::Error;

use structopt::StructOpt;
use atty::Stream;

use comrak::nodes::NodeValue;

use crate::command::{Command, CommandInterpreter, CommandInterpreterError};
use crate::config::{Config, FileConfig};
use crate::{editor, markdown};
use crate::helpers::{base_dir, get_or_insert_with, io_error};
use crate::model::{NoteMetadataStorage};
use crate::querying::{Finder, FindQuery, GitLog, GitContentFetcher, ListDirectory, ListTree, print_list_directory_results, print_note_metadata_results, QueryingError, QueryingResult, RegexMatcher, Searcher, StringMatcher};

pub type RepositoryRef = Rc<RefCell<git2::Repository>>;

pub struct Application {
    config: Config,
    repository: RepositoryRef,
    command_interpreter: CommandInterpreter,
    note_metadata_storage: Option<NoteMetadataStorage>
}

impl Application {
    pub fn new(config: Config) -> Result<Application, AppError> {
        let repository = Rc::new(RefCell::new(open_repository(&config.repository)?));

        Ok(
            Application {
                config: config.clone(),
                repository: repository.clone(),
                command_interpreter: CommandInterpreter::new(config, repository),
                note_metadata_storage: None
            }
        )
    }

    pub fn run(&mut self, input_command: InputCommand) -> Result<(), AppError> {
        match input_command {
            InputCommand::Initialize { .. } => {
                println!("Not supported in interactive mode.");
            }
            InputCommand::Switch { path } => {
                let repository_path = if path.is_absolute() {
                    path
                } else {
                    base_dir().join(path)
                };

                self.config.repository = repository_path.clone();
                *self.repository.borrow_mut() = open_repository(&self.config.repository)?;
                self.command_interpreter = CommandInterpreter::new(self.config.clone(), self.repository.clone());
                self.clear_cache();

                let config_file = &base_dir().join("config.toml");
                let mut file_config = FileConfig::load(&config_file)?;
                file_config.repository = repository_path;
                file_config.save(config_file)?;

                self.config.print();
            }
            InputCommand::Config { only_repository } => {
                if only_repository {
                    println!("{}", self.config.repository.to_str().unwrap());
                } else {
                    self.config.print();
                }
            }
            InputCommand::Add { path, tags } => {
                if atty::is(Stream::Stdin) {
                    self.command_interpreter.execute(vec![
                        Command::AddNote { path, tags },
                        Command::Commit
                    ])?;
                } else {
                    let mut content = String::new();
                    stdin().read_to_string(&mut content)?;
                    self.command_interpreter.execute(vec![
                        Command::AddNoteWithContent { path, tags, content },
                        Command::Commit
                    ])?;
                }

                self.clear_cache();
            }
            InputCommand::Edit { path, clear_tags, add_tags } => {
                if atty::is(Stream::Stdin) {
                    self.command_interpreter.execute(vec![
                        Command::EditNoteContent { path, clear_tags, add_tags },
                        Command::Commit
                    ])?;
                } else {
                    let mut content = String::new();
                    stdin().read_to_string(&mut content)?;
                    self.command_interpreter.execute(vec![
                        Command::EditNoteSetContent { path, clear_tags, add_tags, content },
                        Command::Commit
                    ])?;
                }

                self.clear_cache();
            }
            InputCommand::Move { source, destination, force } => {
                self.command_interpreter.execute(vec![
                    Command::MoveNote { source, destination, force },
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
            InputCommand::PrintContent { path, git_reference, only_code, only_output } => {
                let content = self.get_note_content(&path, git_reference)?;

                if only_code || only_output {
                    let arena = markdown::storage();
                    let root = markdown::parse(&arena, &content);

                    markdown::visit_code_blocks::<CommandInterpreterError, _>(
                        &root,
                        |current_node| {
                            if let NodeValue::CodeBlock(ref block) = current_node.data.borrow().value {
                                print!("{}", block.literal);
                            }

                            Ok(())
                        },
                        only_code,
                        only_output
                    )?;
                } else {
                    print!("{}", content);
                }
            }
            InputCommand::Show { path, git_reference, only_code, only_output } => {
                let content = self.get_note_content(&path, git_reference)?;

                if only_code || only_output {
                    let arena = markdown::storage();
                    let root = markdown::parse(&arena, &content);

                    let mut new_content = String::new();
                    markdown::visit_code_blocks::<CommandInterpreterError, _>(
                        &root,
                        |current_node| {
                            if let NodeValue::CodeBlock(ref block) = current_node.data.borrow().value {
                                new_content += &block.literal;
                            }

                            Ok(())
                        },
                        only_code,
                        only_output
                    )?;

                    editor::launch_with_content(&self.config, &new_content)?;
                } else {
                    editor::launch_with_content(&self.config, &content)?;
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
            InputCommand::Search { mut query, case_sensitive, history } => {
                if !case_sensitive {
                    query = format!("(?i)({})", query);
                }
                let query = Regex::new(&query)?;

                self.note_metadata_storage()?;
                let searcher = Searcher::new(self.note_metadata_storage_ref()?)?;

                if history.len() == 0 {
                    searcher.search(&query)?;
                } else if history.len() == 2 {
                    searcher.search_historic(
                        self.repository.borrow().deref(),
                        &query,
                        &history[0],
                        &history[1]
                    )?;
                } else {
                    return Err(AppError::Input("Expected two arguments".to_owned()));
                }
            }
            InputCommand::Log { count } => {
                let repository = self.repository.borrow();
                let git_log = GitLog::new(repository.deref(), count)?;
                git_log.print()?;
            }
        }

        Ok(())
    }

    pub fn execute_commands(&mut self, commands: Vec<Command>) -> Result<(), AppError> {
        self.command_interpreter.execute(commands)?;
        self.clear_cache();
        Ok(())
    }

    fn get_note_content(&mut self, path: &Path, git_reference: Option<String>) -> QueryingResult<String> {
        if let Some(git_reference) = git_reference {
            self.note_metadata_storage()?;
            let repository = self.repository.borrow();
            let git_content_fetcher = GitContentFetcher::new(
                repository.deref(),
                self.note_metadata_storage_ref()?
            )?;

            if let Some(commit_content) = git_content_fetcher.fetch(&path, &git_reference)? {
                Ok(commit_content)
            } else {
                Err(QueryingError::NoteNotFoundAtGitReference(git_reference))
            }
        } else {
            Ok(self.note_metadata_storage()?.get_content(&path)?)
        }
    }

    fn clear_cache(&mut self) {
        self.note_metadata_storage = None;
    }

    fn note_metadata_storage(&mut self) -> std::io::Result<&NoteMetadataStorage> {
        get_or_insert_with(
            &mut self.note_metadata_storage,
            || Ok(NoteMetadataStorage::from_dir(&self.config.repository)?)
        ).map(|x| &*x)
    }

    fn note_metadata_storage_ref(&self) -> std::io::Result<&NoteMetadataStorage> {
        self.note_metadata_storage.as_ref().ok_or_else(|| io_error("note_metadata_storage not created"))
    }
}

#[derive(StructOpt)]
#[structopt(about="CLI & Git based notes/snippet application")]
pub struct MainInputCommand {
    #[structopt(subcommand)]
    pub command: Option<InputCommand>,
}

#[derive(Debug, StructOpt)]
#[structopt(global_setting=structopt::clap::AppSettings::AllowNegativeNumbers)]
pub enum InputCommand {
    /// Creates a new repository. Also creates config file if it doesn't exist.
    #[structopt(name="init")]
    Initialize {
        name: String
    },
    /// Switches the active repository to the given one. If path is relative, then it is relative to $HOME/.gitnotes
    Switch {
        path: PathBuf
    },
    /// Prints the active config
    Config {
        #[structopt(long="repo")]
        only_repository: bool,
    },
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
        /// Deletes note if it exists at destination
        #[structopt(long, short)]
        force: bool
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
        /// Prints the content at the given git reference
        #[structopt(long="ref")]
        git_reference: Option<String>,
        /// Print only code content.
        #[structopt(long="code")]
        only_code: bool,
        /// Print only output content.
        #[structopt(long="output")]
        only_output: bool
    },
    /// Shows the content of a note in an editor
    Show {
        /// The absolute path of the note. Id also work.
        path: PathBuf,
        /// Prints the content at the given git reference
        #[structopt(long="ref")]
        git_reference: Option<String>,
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
        case_sensitive: bool,
        /// Search through git history (reverse) instead between the given references (inclusive)
        #[structopt(long)]
        history: Vec<String>
    },
    /// Lists git commits
    Log {
        /// The number of commits to show. -1 for all.
        #[structopt(default_value="5")]
        count: isize
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
    #[error("Failed to open repository: {0}")]
    FailedToOpenRepository(git2::Error),

    #[error("{0}")]
    Command(CommandInterpreterError),

    #[error("{0}")]
    Querying(QueryingError),

    #[error("Input error: {0}")]
    Input(String),

    #[error("{0}")]
    Regex(regex::Error),

    #[error("{0}")]
    Git(git2::Error),

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

impl From<git2::Error> for AppError {
    fn from(err: git2::Error) -> Self {
        AppError::Git(err)
    }
}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::IO(err)
    }
}

fn open_repository(path: &Path) -> Result<git2::Repository, AppError> {
    git2::Repository::open(path).map_err(|err| AppError::FailedToOpenRepository(err))
}

#[test]
fn test_add_and_run_snippet() {
    use tempfile::TempDir;

    let temp_repository_dir = TempDir::new().unwrap();
    let config = Config::from_env(FileConfig::new(&temp_repository_dir.path().to_path_buf()));
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

    app.execute_commands(vec![
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

    app.execute_commands(vec![
        Command::EditNoteSetContent {
            path: note_path.to_path_buf(),
            clear_tags: false,
            add_tags: vec![],
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
    use tempfile::TempDir;

    let temp_repository_dir = TempDir::new().unwrap();
    let config = Config::from_env(FileConfig::new(&temp_repository_dir.path().to_path_buf()));
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

    app.execute_commands(vec![
        Command::AddNoteWithContent {
            path: note_path.to_path_buf(),
            tags: vec!["python".to_owned()],
            content: note_content.clone()
        },
        Command::Commit
    ]).unwrap();
    assert_eq!(note_content, app.note_metadata_storage().unwrap().get_content(note_path).unwrap());
    assert_eq!(1, repository.reflog("HEAD").unwrap().len());

    app.run(InputCommand::Move { source: note_path.to_owned(), destination: note_path2.to_owned(), force: false }).unwrap();
    assert_eq!(false, app.note_metadata_storage().unwrap().get_content(note_path).is_ok());
    assert_eq!(note_content, app.note_metadata_storage().unwrap().get_content(note_path2).unwrap());
    assert_eq!(2, repository.reflog("HEAD").unwrap().len());
}

#[test]
fn test_add_and_move_to_existing1() {
    use tempfile::TempDir;

    let temp_repository_dir = TempDir::new().unwrap();
    let config = Config::from_env(FileConfig::new(&temp_repository_dir.path().to_path_buf()));
    let repository = git2::Repository::init(&config.repository).unwrap();

    let note_path = Path::new("2023/07/sample.py");
    let note_path2 = Path::new("2023/07/01/sample.py");
    let note_content = "Hello, World #1".to_owned();
    let note_content2 = "Hello, World #2".to_owned();

    let mut app = Application::new(config).unwrap();

    app.execute_commands(vec![
        Command::AddNoteWithContent {
            path: note_path.to_path_buf(),
            tags: vec!["python".to_owned()],
            content: note_content.clone()
        },
        Command::AddNoteWithContent {
            path: note_path2.to_path_buf(),
            tags: vec!["python".to_owned()],
            content: note_content2.clone()
        },
        Command::Commit
    ]).unwrap();
    let note_id = app.note_metadata_storage().unwrap().get_id(note_path).unwrap();
    let note_id2 = app.note_metadata_storage().unwrap().get_id(note_path2).unwrap();
    assert_eq!(note_content, app.note_metadata_storage().unwrap().get_content(note_path).unwrap());
    assert_eq!(note_content2, app.note_metadata_storage().unwrap().get_content(note_path2).unwrap());
    assert_eq!(1, repository.reflog("HEAD").unwrap().len());

    let err = app.run(InputCommand::Move { source: note_path.to_owned(), destination: note_path2.to_owned(), force: false }).err().unwrap();
    if let AppError::Command(CommandInterpreterError::NoteAtDestination(err_path)) = err {
        assert_eq!(note_path2, err_path);
        assert_eq!(note_id, app.note_metadata_storage().unwrap().get_id(note_path).unwrap());
        assert_eq!(note_id2, app.note_metadata_storage().unwrap().get_id(note_path2).unwrap());
    } else {
        assert!(false, "Expected 'NoteAtDestination' error");
    }
}

#[test]
fn test_add_and_move_to_existing2() {
    use tempfile::TempDir;

    let temp_repository_dir = TempDir::new().unwrap();
    let config = Config::from_env(FileConfig::new(&temp_repository_dir.path().to_path_buf()));
    let repository = git2::Repository::init(&config.repository).unwrap();

    let note_path = Path::new("2023/07/sample.py");
    let note_path2 = Path::new("2023/07/01/sample.py");
    let note_content = "Hello, World #1".to_owned();
    let note_content2 = "Hello, World #2".to_owned();

    let mut app = Application::new(config).unwrap();

    app.execute_commands(vec![
        Command::AddNoteWithContent {
            path: note_path.to_path_buf(),
            tags: vec!["python".to_owned()],
            content: note_content.clone()
        },
        Command::AddNoteWithContent {
            path: note_path2.to_path_buf(),
            tags: vec!["python".to_owned()],
            content: note_content2.clone()
        },
        Command::Commit
    ]).unwrap();
    let note_id = app.note_metadata_storage().unwrap().get_id(note_path).unwrap();
    assert_eq!(note_content, app.note_metadata_storage().unwrap().get_content(note_path).unwrap());
    assert_eq!(note_content2, app.note_metadata_storage().unwrap().get_content(note_path2).unwrap());
    assert_eq!(1, repository.reflog("HEAD").unwrap().len());

    app.run(InputCommand::Move { source: note_path.to_owned(), destination: note_path2.to_owned(), force: true }).unwrap();
    assert_eq!(false, app.note_metadata_storage().unwrap().get_content(note_path).is_ok());
    assert_eq!(note_content, app.note_metadata_storage().unwrap().get_content(note_path2).unwrap());
    assert_eq!(note_id, app.note_metadata_storage().unwrap().get(note_path2).unwrap().id);
    assert_eq!(2, repository.reflog("HEAD").unwrap().len());
}

#[test]
fn test_add_and_remove() {
    use tempfile::TempDir;

    let temp_repository_dir = TempDir::new().unwrap();
    let config = Config::from_env(FileConfig::new(&temp_repository_dir.path().to_path_buf()));
    let repository = git2::Repository::init(&config.repository).unwrap();

    let note_path = Path::new("2023/07/sample.py");
    let note_content = r#"Hello, World!

``` python
import numpy as np
print(np.square(np.arange(0, 10)))
```
"#.to_string();

    let mut app = Application::new(config).unwrap();

    app.execute_commands(vec![
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

#[test]
fn test_add_and_change_tags() {
    use tempfile::TempDir;

    let temp_repository_dir = TempDir::new().unwrap();
    let config = Config::from_env(FileConfig::new(&temp_repository_dir.path().to_path_buf()));
    let repository = git2::Repository::init(&config.repository).unwrap();

    let note_path = Path::new("2023/07/sample.py");
    let note_content = r#"Hello, World!

``` python
import numpy as np
print(np.square(np.arange(0, 10)))
```
"#.to_string();

    let mut app = Application::new(config).unwrap();

    app.execute_commands(vec![
        Command::AddNoteWithContent {
            path: note_path.to_path_buf(),
            tags: vec!["python".to_owned()],
            content: note_content.clone()
        },
        Command::Commit
    ]).unwrap();
    assert_eq!(note_content, app.note_metadata_storage().unwrap().get_content(note_path).unwrap());
    assert_eq!(1, repository.reflog("HEAD").unwrap().len());

    app.execute_commands(vec![
        Command::EditNoteSetContent {
            path: note_path.to_path_buf(),
            clear_tags: false,
            add_tags: vec!["snippet".to_owned()],
            content: note_content.clone()
        },
        Command::Commit
    ]).unwrap();
    assert_eq!(note_content, app.note_metadata_storage().unwrap().get_content(note_path).unwrap());
    assert_eq!(vec!["python".to_owned(), "snippet".to_owned()], app.note_metadata_storage().unwrap().get(note_path).unwrap().tags);
    assert_eq!(2, repository.reflog("HEAD").unwrap().len());
}
