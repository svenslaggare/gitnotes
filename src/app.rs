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
use crate::config::{Config, config_path, FileConfig};
use crate::{editor, interactive, markdown, querying};
use crate::helpers::{base_dir, get_or_insert_with, io_error};
use crate::model::{NoteFileTreeCreateConfig, NoteMetadataStorage};
use crate::querying::{Finder, FindQuery, GitLog, ListDirectory, ListTree, print_list_directory_results, print_note_metadata_results, QueryingError, QueryingResult, RegexMatcher, Searcher, StringMatcher};

pub type RepositoryRef = Rc<RefCell<git2::Repository>>;

pub struct Application {
    config: Config,
    repository: RepositoryRef,
    command_interpreter: CommandInterpreter,
    note_metadata_storage: Option<NoteMetadataStorage>,
    auto_commit: bool
}

impl Application {
    pub fn new(config: Config) -> Result<Application, AppError> {
        let repository = Rc::new(RefCell::new(open_repository(&config.repository)?));

        Ok(
            Application {
                config: config.clone(),
                repository: repository.clone(),
                command_interpreter: CommandInterpreter::new(config, repository)?,
                note_metadata_storage: None,
                auto_commit: true
            }
        )
    }

    pub fn run(&mut self, input_command: InputCommand) -> Result<Option<InputCommand>, AppError> {
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
                self.command_interpreter = CommandInterpreter::new(self.config.clone(), self.repository.clone())?;
                self.clear_cache();

                let mut file_config = FileConfig::load(&config_path())?;
                file_config.repository = repository_path;
                file_config.save(&config_path())?;

                self.config.print();
            }
            InputCommand::Config { only_repository, set } => {
                if let Some(set) = set {
                    let parts = set.split("=").collect::<Vec<_>>();
                    if let &[key, value] = &parts[..] {
                        let mut file_config = FileConfig::load(&config_path())?;
                        file_config.change(key, value).map_err(|err| AppError::Input(err))?;
                        file_config.save(&config_path())?;

                        self.config = Config::from_env(file_config);
                        self.config.print();
                    } else {
                        return Err(AppError::Input(format!("Format: key=value")));
                    }
                } else {
                    if only_repository {
                        println!("{}", self.config.repository.to_str().unwrap());
                    } else {
                        self.config.print();
                    }
                }
            }
            InputCommand::Add { path, tags } => {
                let path = self.get_path(path)?;

                if atty::is(Stream::Stdin) {
                    self.create_and_execute_commands(vec![
                        Command::AddNote { path, tags }
                    ])?;
                } else {
                    let mut content = String::new();
                    stdin().read_to_string(&mut content)?;
                    self.create_and_execute_commands(vec![
                        Command::AddNoteWithContent { path, tags, content }
                    ])?;
                }
            }
            InputCommand::Edit { path, clear_tags, add_tags } => {
                let path = self.get_path(path)?;

                if atty::is(Stream::Stdin) {
                    self.create_and_execute_commands(vec![
                        Command::EditNoteContent { path, clear_tags, add_tags }
                    ])?;
                } else {
                    let mut content = String::new();
                    stdin().read_to_string(&mut content)?;
                    self.create_and_execute_commands(vec![
                        Command::EditNoteSetContent { path, clear_tags, add_tags, content }
                    ])?;
                }
            }
            InputCommand::Move { source, destination, force } => {
                let source = self.get_path(source)?;
                let destination = self.get_path(destination)?;

                self.create_and_execute_commands(vec![
                    Command::MoveNote { source, destination, force }
                ])?;
            }
            InputCommand::Remove { path } => {
                let path = self.get_path(path)?;

                self.create_and_execute_commands(vec![
                    Command::RemoveNote { path }
                ])?;
            }
            InputCommand::RunSnippet { path, save_output } => {
                let path = self.get_path(path)?;

                let mut commands = vec![
                    Command::RunSnippet { path, save_output }
                ];

                if save_output && self.auto_commit {
                    commands.push(Command::Commit);
                }

                self.execute_commands(commands)?;
            }
            InputCommand::Begin { } => {
                self.auto_commit = false;
            }
            InputCommand::Commit { } => {
                self.execute_commands(vec![Command::Commit])?;
                self.auto_commit = true;
            }
            InputCommand::PrintContent { path, history, only_code, only_output } => {
                let path = self.get_path(path)?;

                let content = self.get_note_content(&path, history)?;

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
            InputCommand::Show { path, history, only_code, only_output } => {
                let path = self.get_path(path)?;

                let content = self.get_note_content(&path, history)?;

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
                let query = query.unwrap_or_else(|| Path::new("").to_owned());
                let query = self.get_path(query)?;

                let list_directory = ListDirectory::new(self.note_metadata_storage()?)?;
                let results = list_directory.list(&query)?;
                print_list_directory_results(&results)?
            }
            InputCommand::Tree { prefix, using_date, using_tags, } => {
                let prefix = prefix.unwrap_or_else(|| Path::new("").to_owned());
                let prefix = self.get_path(prefix)?;

                let mut create_config = NoteFileTreeCreateConfig::default();
                create_config.using_date = using_date;
                create_config.using_tags = using_tags;

                let list_tree = ListTree::new(self.note_metadata_storage()?, create_config)?;
                list_tree.list(&prefix);
            }
            InputCommand::Finder { interactive, command } => {
                let query = match command {
                    InputCommandFinder::Tags { tags } => {
                        FindQuery::Tags(tags)
                    }
                    InputCommandFinder::Name { name } => {
                        FindQuery::Path(name)
                    }
                    InputCommandFinder::Id { id } => {
                        FindQuery::Id(id)
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

                if let Some(command) = interactive {
                    if let Some(next_command) = interactive::select_with_note_metadata(&command, &results)? {
                        return Ok(Some(next_command));
                    }
                }
            }
            InputCommand::SearchContent { mut query, case_sensitive, history, interactive } => {
                if !case_sensitive {
                    query = format!("(?i)({})", query);
                }
                let query = Regex::new(&query)?;

                self.note_metadata_storage()?;
                let searcher = Searcher::new(self.note_metadata_storage_ref()?)?;

                if history.len() == 0 {
                    let matches = searcher.search(&query)?;
                    if let Some(command) = interactive {
                        if let Some(next_command) = interactive::select_with_note_metadata(&command, &matches)? {
                            return Ok(Some(next_command));
                        }
                    }
                } else if history.len() == 2 {
                    let matches = searcher.search_historic(
                        self.repository.borrow().deref(),
                        &query,
                        &history[0],
                        &history[1]
                    )?;

                    if let Some(command) = interactive {
                        let next_command = interactive::select(&command, matches.len(), |command_name: &str, index: usize| {
                            format!("{} --history {} {}", command_name, matches[index].0, matches[index].1.path.to_str().unwrap())
                        })?;

                        if let Some(next_command) = next_command{
                            return Ok(Some(next_command));
                        }
                    }
                } else {
                    return Err(AppError::Input("Expected two arguments".to_owned()));
                }
            }
            InputCommand::Log { count } => {
                let repository = self.repository.borrow();
                let git_log = GitLog::new(repository.deref(), count)?;
                git_log.print()?;
            }
            InputCommand::Info { path, only_file_system_path } => {
                self.note_metadata_storage()?;
                let note_metadata = self.note_metadata_storage_ref()?.get(&path).ok_or_else(|| QueryingError::NoteNotFound(path.to_str().unwrap().to_owned()))?;
                let file_system_path = NoteMetadataStorage::get_note_storage_path(&self.config.repository, &note_metadata.id).1.to_str().unwrap().to_owned();

                if !only_file_system_path {
                    println!("Id: {}", note_metadata.id);
                    println!("Path: {}", note_metadata.path.to_str().unwrap());
                    println!("File system path: {}", file_system_path);
                    println!("Tags: {}", note_metadata.tags.join(", "));
                    println!("Created: {}", note_metadata.created);
                    println!("Last updated: {}", note_metadata.last_updated);
                } else {
                    println!("{}", file_system_path);
                }
            }
            InputCommand::WebEditor { .. } => {
                println!("Not supported in interactive mode.");
            }
        }

        Ok(None)
    }

    pub fn run_until_completion(&mut self, input_command: InputCommand) -> Result<(), AppError> {
        let mut next_input_command = Some(input_command);
        while let Some(input_command) = next_input_command {
            next_input_command = self.run(input_command)?;
        }

        Ok(())
    }

    pub fn execute_commands(&mut self, commands: Vec<Command>) -> Result<(), AppError> {
        self.command_interpreter.execute(commands)?;
        self.clear_cache();
        Ok(())
    }

    pub fn create_commands(&self, mut commands: Vec<Command>) -> Vec<Command> {
        if self.auto_commit {
            commands.push(Command::Commit);
        }

        commands
    }

    pub fn create_and_execute_commands(&mut self, commands: Vec<Command>) -> Result<(), AppError> {
        self.execute_commands(self.create_commands(commands))
    }

    pub fn note_metadata_storage(&mut self) -> std::io::Result<&NoteMetadataStorage> {
        get_or_insert_with(
            &mut self.note_metadata_storage,
            || Ok(NoteMetadataStorage::from_dir(&self.config.repository)?)
        ).map(|x| &*x)
    }

    pub fn note_metadata_storage_ref(&self) -> std::io::Result<&NoteMetadataStorage> {
        self.note_metadata_storage.as_ref().ok_or_else(|| io_error("note_metadata_storage not created"))
    }

    fn get_note_content(&mut self, path: &Path, git_reference: Option<String>) -> QueryingResult<String> {
        self.note_metadata_storage()?;
        let repository = self.repository.borrow();
        querying::get_note_content(
            repository.deref(),
            self.note_metadata_storage_ref()?,
            path,
            git_reference
        )
    }

    fn clear_cache(&mut self) {
        self.note_metadata_storage = None;
    }

    fn get_path(&mut self, path: PathBuf) -> Result<PathBuf, AppError> {
        self.note_metadata_storage()?;
        self.note_metadata_storage_ref()?.resolve_path(
            path,
            self.config.use_real.clone(),
            self.config.real_base_dir.as_ref().map(|p| p.as_path())
        ).map_err(|err| AppError::InvalidPath(err))
    }
}

#[derive(StructOpt)]
#[structopt(about="CLI based notes & snippet application powered by Git.")]
pub struct MainInputCommand {
    /// Use real working directory
    #[structopt(long="real")]
    pub use_real: bool,
    /// Don't use real working directory
    #[structopt(long="no-real")]
    pub use_non_real: bool,
    #[structopt(subcommand)]
    pub command: Option<InputCommand>
}

pub struct MainInputConfig {
    pub use_real: bool,
    pub use_non_real: bool,
}

impl MainInputConfig {
    pub fn from_input(input: &MainInputCommand) -> MainInputConfig {
        MainInputConfig {
            use_real: input.use_real,
            use_non_real: input.use_non_real,
        }
    }

    pub fn apply(&self, mut config: Config) -> Config {
        if self.use_real {
            config.use_real = true;
        }

        if self.use_non_real {
            config.use_real = false;
        }

        config
    }
}

#[derive(Debug, StructOpt)]
#[structopt(global_setting=structopt::clap::AppSettings::AllowNegativeNumbers)]
pub enum InputCommand {
    /// Creates a new repository. Also creates config file if it doesn't exist.
    #[structopt(name="init")]
    Initialize {
        /// The name of the repository
        name: String,
        /// The name refers to an existing git repository.
        #[structopt(long)]
        use_existing: bool
    },
    /// Switches the active repository to the given one. If path is relative, then it is relative to $HOME/.gitnotes
    Switch {
        path: PathBuf
    },
    /// Prints the active config
    Config {
        /// Prints only the name of the repository.
        #[structopt(long="repo")]
        only_repository: bool,
        /// Sets the given config value (format key=value).
        #[structopt(long)]
        set: Option<String>
    },
    /// Create a new note.
    Add {
        /// The path of the note.
        path: PathBuf,
        /// The tags of the note.
        #[structopt(long)]
        tags: Vec<String>
    },
    /// Edit an existing note.
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
    /// Begins a commit. All subsequent operations are done within this commit (interactive mode only).
    Begin {

    },
    /// Commits the started transaction. If no changes have been made, a commit is not created (interactive mode only).
    Commit {

    },
    /// Prints the content of a note.
    #[structopt(name="cat")]
    PrintContent {
        /// The absolute path of the note. Id also work.
        path: PathBuf,
        /// Prints the content at the given git commit
        #[structopt(long="history")]
        history: Option<String>,
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
        /// Prints the content at the given git commit
        #[structopt(long="history")]
        history: Option<String>,
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
        query: Option<PathBuf>
    },
    /// Lists note in a tree structure.
    Tree {
        /// List tree starting at the given prefix.
        prefix: Option<PathBuf>,
        /// Uses creation date as the path instead (file name is still used)
        #[structopt(long="date", short="-d")]
        using_date: bool,
        /// Uses tags as the path instead (file name is still used)
        #[structopt(long="tags", short="-t")]
        using_tags: bool
    },
    /// Searches for note based on properties.
    #[structopt(name="find")]
    Finder {
        /// Creates an interactive prompt to choose which match to launch a new command with
        #[structopt(long, short)]
        interactive: Option<String>,
        #[structopt(subcommand)]
        command: InputCommandFinder
    },
    /// Searches for note based on content.
    #[structopt(name="grep")]
    SearchContent {
        /// The regex query.
        query: String,
        /// Indicates if the match is cans sensitive
        #[structopt(long="no-ignore-case")]
        case_sensitive: bool,
        /// Search through git history (reverse) instead between the given references (inclusive)
        #[structopt(long)]
        history: Vec<String>,
        /// Creates an interactive prompt to choose which match to launch a new command with
        #[structopt(long, short)]
        interactive: Option<String>
    },
    /// Lists git commits
    Log {
        /// The number of commits to show. -1 for all.
        #[structopt(default_value="5")]
        count: isize
    },
    /// Shows information about a note
    Info {
        /// The absolute path of the note. Id also work.
        path: PathBuf,
        /// Prints only the file system path.
        #[structopt(long="file-system")]
        only_file_system_path: bool,
    },
    /// Runs web editor in stand alone mode (use web-editor in editor config to use it)
    WebEditor {
        /// The (file system) path to edit
        path: PathBuf,
        /// The part to run the web server at (default: 9000)
        #[structopt(long, default_value="9000")]
        port: u16
    }
}

#[derive(Debug, StructOpt)]
pub enum InputCommandFinder {
    /// Searches based on tags.
    Tags {
        /// The tags that the note must contain (AND).
        tags: Vec<StringMatcher>
    },
    /// Searches based on name.
    Name {
        /// Regex pattern.
        name: RegexMatcher
    },
    /// Searches based on id.
    Id {
        /// Regex pattern.
        id: RegexMatcher
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

    #[error("Invalid path: {0}")]
    InvalidPath(String),

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
fn test_add() {
    use tempfile::TempDir;

    let temp_repository_dir = TempDir::new().unwrap();
    let config = Config::from_env(FileConfig::new(&temp_repository_dir.path().to_path_buf()));
    let repository = git2::Repository::init(&config.repository).unwrap();

    let note_path = Path::new("2023/07/sample.py");
    let note_content = r#"Hello, World!

``` python
xs = list(range(0, 10))
print([x * x for x in xs])
```
"#.to_string();

    let mut app = Application::new(config).unwrap();

    app.create_and_execute_commands(vec![
        Command::AddNoteWithContent {
            path: note_path.to_path_buf(),
            tags: vec![],
            content: note_content.clone()
        },
    ]).unwrap();
    assert_eq!(note_content, app.note_metadata_storage().unwrap().get_content(note_path).unwrap());
    assert_eq!(1, repository.reflog("HEAD").unwrap().len());
    assert_eq!(vec!["snippet".to_owned(), "python".to_owned()], app.note_metadata_storage().unwrap().get(note_path).unwrap().tags);
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
xs = list(range(0, 10))
print([x * x for x in xs])
```
"#.to_string();

    let note_content_output = r#"Hello, World\!

``` python
xs = list(range(0, 10))
print([x * x for x in xs])
```

``` output
[0, 1, 4, 9, 16, 25, 36, 49, 64, 81]
```
"#.to_string();

    let note_content2 = r#"Hello, World!

``` python
xs = list(range(0, 11))
print([x * x for x in xs])
```

``` output
[0, 1, 4, 9, 16, 25, 36, 49, 64, 81]
```
"#.to_string();

    let note_content_output2 = r#"Hello, World\!

``` python
xs = list(range(0, 11))
print([x * x for x in xs])
```

``` output
[0, 1, 4, 9, 16, 25, 36, 49, 64, 81, 100]
```
"#.to_string();

    let mut app = Application::new(config).unwrap();

    app.create_and_execute_commands(vec![
        Command::AddNoteWithContent {
            path: note_path.to_path_buf(),
            tags: vec!["python".to_owned()],
            content: note_content.clone()
        }
    ]).unwrap();
    assert_eq!(note_content, app.note_metadata_storage().unwrap().get_content(note_path).unwrap());
    assert_eq!(1, repository.reflog("HEAD").unwrap().len());

    app.run(InputCommand::RunSnippet { path: note_path.to_owned(), save_output: true }).unwrap();
    assert_eq!(note_content_output, app.note_metadata_storage().unwrap().get_content(note_path).unwrap());
    assert_eq!(2, repository.reflog("HEAD").unwrap().len());

    app.execute_commands(app.create_commands(vec![
        Command::EditNoteSetContent {
            path: note_path.to_path_buf(),
            clear_tags: false,
            add_tags: vec![],
            content: note_content2.clone()
        }
    ])).unwrap();
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

    app.create_and_execute_commands(vec![
        Command::AddNoteWithContent {
            path: note_path.to_path_buf(),
            tags: vec!["python".to_owned()],
            content: note_content.clone()
        }
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

    app.create_and_execute_commands(vec![
        Command::AddNoteWithContent {
            path: note_path.to_path_buf(),
            tags: vec!["python".to_owned()],
            content: note_content.clone()
        },
        Command::AddNoteWithContent {
            path: note_path2.to_path_buf(),
            tags: vec!["python".to_owned()],
            content: note_content2.clone()
        }
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

    app.create_and_execute_commands(vec![
        Command::AddNoteWithContent {
            path: note_path.to_path_buf(),
            tags: vec!["python".to_owned()],
            content: note_content.clone()
        },
        Command::AddNoteWithContent {
            path: note_path2.to_path_buf(),
            tags: vec!["python".to_owned()],
            content: note_content2.clone()
        }
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

    app.create_and_execute_commands(vec![
        Command::AddNoteWithContent {
            path: note_path.to_path_buf(),
            tags: vec!["python".to_owned()],
            content: note_content.clone()
        }
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

    app.create_and_execute_commands(vec![
        Command::AddNoteWithContent {
            path: note_path.to_path_buf(),
            tags: vec!["python".to_owned()],
            content: note_content.clone()
        }
    ]).unwrap();
    assert_eq!(note_content, app.note_metadata_storage().unwrap().get_content(note_path).unwrap());
    assert_eq!(1, repository.reflog("HEAD").unwrap().len());

    app.create_and_execute_commands(vec![
        Command::EditNoteSetContent {
            path: note_path.to_path_buf(),
            clear_tags: false,
            add_tags: vec!["snippet".to_owned()],
            content: note_content.clone()
        }
    ]).unwrap();
    assert_eq!(note_content, app.note_metadata_storage().unwrap().get_content(note_path).unwrap());
    assert_eq!(vec!["python".to_owned(), "snippet".to_owned()], app.note_metadata_storage().unwrap().get(note_path).unwrap().tags);
    assert_eq!(2, repository.reflog("HEAD").unwrap().len());
}
