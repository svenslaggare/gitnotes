use std::cell::RefCell;
use std::io::{IsTerminal, stdin};
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::rc::Rc;

use globset::Glob;

use regex::Regex;
use thiserror::Error;

use git2::{FetchOptions, PushOptions, RemoteCallbacks};

use structopt::StructOpt;

use crate::command::{Command, CommandInterpreter, CommandError, CommandResult};
use crate::config::{Config, config_path, FileConfig};
use crate::{editor, git_helpers, interactive, markdown, querying};
use crate::helpers::{base_dir, get_or_insert_with, io_error, StdinExt};
use crate::model::{NoteFileTree, NoteFileTreeCreateConfig, NoteMetadataStorage, NOTES_DIR};
use crate::querying::{Finder, FindQuery, GitLog, ListDirectory, ListTree, print_list_directory_results, print_note_metadata_results, QueryingError, QueryingResult, RegexMatcher, Searcher, StringMatcher};
use crate::web_editor::AccessMode;

pub type RepositoryRef = Rc<RefCell<git2::Repository>>;

pub struct App {
    config: Config,
    repository: RepositoryRef,
    command_interpreter: CommandInterpreter,
    note_metadata_storage: Option<NoteMetadataStorage>,
    auto_commit: bool,
    working_dir: Option<PathBuf>,
    version: u64
}

impl App {
    pub fn new(config: Config) -> AppResult<App> {
        App::with_custom(config, |config, repository| CommandInterpreter::new(config, repository))
    }

    pub fn with_custom<F: FnOnce(Config, RepositoryRef) -> CommandResult<CommandInterpreter>>(config: Config, create_ci: F) -> AppResult<App> {
        let repository = Rc::new(RefCell::new(open_repository(&config.repository)?));

        let notes_dir = config.repository.join(NOTES_DIR);
        if !notes_dir.exists() {
            std::fs::create_dir_all(notes_dir)?;
        }

        Ok(
            App {
                config: config.clone(),
                repository: repository.clone(),
                command_interpreter: create_ci(config.clone(), repository)?,
                note_metadata_storage: None,
                auto_commit: true,
                working_dir: get_initial_working_dir(&config),
                version: 0
            }
        )
    }

    pub fn run(&mut self, input_command: InputCommand) -> AppResult<Option<InputCommand>> {
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

                let mut file_config = FileConfig::load(&config_path())?;
                file_config.repository = repository_path;
                file_config.save(&config_path())?;

                self.clear_cache();

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
                        self.clear_cache();

                        self.config.print();
                    } else {
                        return Err(AppError::Input("Format: key=value".to_string()));
                    }
                } else {
                    if only_repository {
                        println!("{}", self.config.repository.to_str().unwrap());
                    } else {
                        self.config.print();
                    }
                }
            }
            InputCommand::UpdateSymbolicLinks {} => {
                self.create_and_execute_commands(vec![
                    Command::UpdateSymbolicLinks {}
                ])?;
            }
            InputCommand::Add { path, tags } => {
                let path = self.get_path(path)?;

                if !self.config.allow_stdin || stdin().is_terminal() {
                    self.create_and_execute_commands(vec![
                        Command::AddNote { path, tags }
                    ])?;
                } else {
                    let content = stdin().read_into_string()?;
                    self.create_and_execute_commands(vec![
                        Command::AddNoteWithContent { path, tags, content }
                    ])?;
                }
            }
            InputCommand::Edit { path, history, clear_tags, add_tags } => {
                let path = self.get_path(path)?;

                if !self.config.allow_stdin || stdin().is_terminal() {
                    self.create_and_execute_commands(vec![
                        Command::EditNoteContent { path, history, clear_tags, add_tags }
                    ])?;
                } else {
                    if history.is_some() {
                        return Err(AppError::Input("History not supported when using stdin as input".to_owned()));
                    }

                    let content = stdin().read_into_string()?;
                    self.create_and_execute_commands(vec![
                        Command::EditNoteSetContent { path, clear_tags, add_tags, content }
                    ])?;
                }
            }
            InputCommand::Move { source, destination, force } => {
                let working_dir = self.working_dir()?;
                let source = self.get_path(source)?;
                let destination = self.get_path(destination)?;

                self.note_metadata_storage()?;

                let result = self.create_and_execute_commands(self.create_move_commands(
                    working_dir,
                    source,
                    destination,
                    force
                )?);

                if let Err(err) = result {
                    self.command_interpreter.reset()?;
                    return Err(err);
                }
            }
            InputCommand::Remove { path, recursive } => {
                let working_dir = self.working_dir()?;
                let path = self.get_path(path)?;

                let result = self.create_and_execute_commands(self.create_remove_commands(
                    working_dir,
                    path,
                    recursive
                )?);

                if let Err(err) = result {
                    self.command_interpreter.reset()?;
                    return Err(err);
                }
            }
            InputCommand::Undo { commit } => {
                self.create_and_execute_commands(vec![
                    Command::UndoCommit { commit }
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
            InputCommand::ConvertFile { path, destination } => {
                let path = self.get_path(path)?;
                let abs_content_path = self.get_note_content_path(&path)?;
                markdown::convert(&abs_content_path, &destination)?;
            }
            InputCommand::Begin { } => {
                self.auto_commit = false;
                self.command_interpreter.new_commit()?;
            }
            InputCommand::Commit { } => {
                self.execute_commands(vec![Command::Commit])?;
                self.auto_commit = true;
            }
            InputCommand::Remote { command } => {
                match command {
                    InputCommandRemote::List { .. } => {
                        let repository = self.repository.borrow();
                        println!("Remotes:");
                        for remote in repository.remotes()?.iter() {
                            if let Some(remote) = remote {
                                let remote = repository.find_remote(&remote).map_err(|_| AppError::RemoteNotFound(remote.to_owned()))?;
                                println!("{}: {}", remote.name().unwrap_or("N/A"), remote.url().unwrap_or("N/A"));
                            }
                        }
                    }
                    InputCommandRemote::Add { name, url } => {
                        let repository = self.repository.borrow();
                        repository.remote_set_url(&name, &url)?;
                        println!("Added remote '{}.", name);
                    }
                    InputCommandRemote::Remove { name } => {
                        let repository = self.repository.borrow();
                        repository.remote_delete(&name).map_err(|_| AppError::RemoteNotFound(name.to_owned()))?;
                        println!("Removed remote '{}.", name);
                    }
                }
            }
            InputCommand::Synchronize { branch, remote, no_pull, no_push } => {
                let branch = branch.unwrap_or_else(|| self.config.sync_default_branch.clone());
                let remote = remote.unwrap_or_else(|| self.config.sync_default_remote.clone());
                let pull = !no_pull;
                let push = !no_push;

                let repository = self.repository.borrow();

                let branch_ref = git_helpers::find_branch_ref(&repository, &branch)?;
                let mut remote = repository.find_remote(&remote).map_err(|_| AppError::RemoteNotFound(remote.clone()))?;

                if pull {
                    println!("Pulling from remote...");

                    let mut fetch_options = FetchOptions::new();
                    let mut callbacks = RemoteCallbacks::new();
                    callbacks.credentials(git_helpers::create_ssh_credentials());
                    fetch_options.remote_callbacks(callbacks);

                    remote.fetch(&[&branch_ref], Some(&mut fetch_options), None)?;
                    let fetch_head = repository.find_reference("FETCH_HEAD")?;
                    let fetch_commit = repository.reference_to_annotated_commit(&fetch_head)?;
                    git_helpers::merge(&repository, &branch, fetch_commit)?;
                }

                if push {
                    println!("Pushing to remote...");

                    let mut push_options = PushOptions::new();
                    let mut callbacks = RemoteCallbacks::new();
                    callbacks.credentials(git_helpers::create_ssh_credentials());
                    push_options.remote_callbacks(callbacks);

                    remote.push(&[&branch_ref], Some(&mut push_options))?;
                }
            }
            InputCommand::PrintContent { path, history, only_code, only_output } => {
                let path = self.get_path(path)?;

                let content = self.get_note_content(&path, history)?;
                let content = querying::extract_content(content, only_code, only_output)?;
                print!("{}", content);
            }
            InputCommand::Show { path, history, only_code, only_output } => {
                let path = self.get_path(path)?;

                let content = self.get_note_content(&path, history)?;
                let content = querying::extract_content(content, only_code, only_output)?;
                editor::launch_with_content(&self.config, &content, Some(&path), AccessMode::Read)?;
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
            InputCommand::OpenNotesInFileExplorer {} => {
                self.create_and_execute_commands(vec![
                    Command::UpdateSymbolicLinks {}
                ])?;

                open::that(&self.config.repository)?;
            }
            InputCommand::Finder { interactive, command } => {
                let finder = Finder::new(self.note_metadata_storage()?)?;
                let results = finder.find(&command.query())?;
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
                } else {
                    let matches = searcher.search_historic(
                        self.repository.borrow().deref(),
                        &query,
                        &history[0],
                        history.get(1).map(|x| x.as_str())
                    )?;

                    if let Some(command) = interactive {
                        let next_command = interactive::select(&command, matches.len(), |command_name: &str, index: usize| {
                            format!("{} --history {} {}", command_name, matches[index].0, matches[index].1.path.to_str().unwrap())
                        })?;

                        if let Some(next_command) = next_command{
                            return Ok(Some(next_command));
                        }
                    }
                }
            }
            InputCommand::Resource { command } => {
                match command {
                    InputCommandResource::Add { path, destination } => {
                        self.create_and_execute_commands(vec![
                            Command::AddResource { path, destination }
                        ])?;
                    }
                    InputCommandResource::List { query, print_absolute } => {
                        let resources_dir = self.config.resources_dir();
                        querying::list_resources(&resources_dir, query, print_absolute)?;
                    }
                    InputCommandResource::Apply { command, resource } => {
                        let full_path = self.config.resources_dir().join(resource).canonicalize()?;

                        let mut result = std::process::Command::new(&command)
                            .arg(full_path)
                            .stdin(Stdio::inherit())
                            .spawn()
                            .map_err(|err| CommandError::SubProcess(err))?;
                        result.wait().map_err(|err| CommandError::SubProcess(err))?;
                    }
                }
            }
            InputCommand::Log { count } => {
                let repository = self.repository.borrow();
                let git_log = GitLog::new(repository.deref(), count)?;
                git_log.print()?;
            }
            InputCommand::Info { path, only_file_system_path } => {
                self.note_metadata_storage()?;
                let note_metadata = self.note_metadata_storage_ref()?
                    .get(&path)
                    .ok_or_else(|| QueryingError::NoteNotFound(path.to_str().unwrap().to_owned()))?;

                let file_system_path = NoteMetadataStorage::get_note_storage_path(
                    &self.config.repository,
                    &note_metadata.id
                ).1.to_str().unwrap().to_owned();

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
            InputCommand::ChangeWorkingDirectory { path } => {
                let new_working_dir = change_working_dir(
                    self.working_dir.as_ref().map(|p| p.as_path()),
                    path
                );

                let note_file_tree = NoteFileTree::from_iter(self.note_metadata_storage()?.notes())
                    .ok_or_else(|| QueryingError::FailedToCreateNoteFileTree)?;
                
                match note_file_tree.find(&new_working_dir)  {
                    Some(working_dir_tree) if working_dir_tree.is_tree() => {
                        self.working_dir = Some(new_working_dir);
                    }
                    Some(_) => {
                        return Err(AppError::ChangeDirectory("The path is not a directory".to_owned()));
                    }
                    None => {
                        return Err(AppError::ChangeDirectory("The path doesn't exist".to_owned()));
                    }
                }
            }
            InputCommand::PrintWorkingDirectory {} => {
                if let Some(working_dir) = self.working_dir.as_ref() {
                    let working_dir = working_dir.to_str().unwrap();
                    if !working_dir.is_empty() {
                        println!("{}", working_dir);
                    } else {
                        println!("(root)");
                    }
                } else {
                    println!("(root)");
                }
            }
            InputCommand::WebEditor { .. } => {
                println!("Not supported in interactive mode.");
            }
        }

        Ok(None)
    }

    pub fn run_until_completion(&mut self, input_command: InputCommand) -> AppResult<()> {
        let mut next_input_command = Some(input_command);
        while let Some(input_command) = next_input_command {
            next_input_command = self.run(input_command)?;
        }

        Ok(())
    }

    pub fn execute_commands(&mut self, commands: Vec<Command>) -> AppResult<()> {
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

    pub fn create_and_execute_commands(&mut self, commands: Vec<Command>) -> AppResult<()> {
        self.execute_commands(self.create_commands(commands))
    }

    pub fn note_metadata_storage(&mut self) -> std::io::Result<&NoteMetadataStorage> {
        get_or_insert_with(
            &mut self.note_metadata_storage,
            || Ok(NoteMetadataStorage::from_dir_with_config(&self.config)?)
        ).map(|x| &*x)
    }

    pub fn note_metadata_storage_ref(&self) -> std::io::Result<&NoteMetadataStorage> {
        self.note_metadata_storage.as_ref().ok_or_else(|| io_error("note_metadata_storage not created"))
    }

    fn create_move_commands(&self,
                            working_dir: PathBuf,
                            source: PathBuf,
                            destination: PathBuf,
                            force: bool) -> QueryingResult<Vec<Command>> {
        let note_file_tree = NoteFileTree::from_iter(self.note_metadata_storage_ref()?.notes());

        let inner = |source: PathBuf, destination: PathBuf| {
            let source_file_tree = note_file_tree.as_ref().map(|note_file_tree| note_file_tree.find(&source)).flatten();
            if let Some(note_file_tree) = source_file_tree {
                if note_file_tree.is_tree() {
                    let mut moves = Vec::new();
                    note_file_tree.walk(|_, parent, name, tree, _| {
                        let path = parent.join(name);
                        if tree.is_leaf() {
                            moves.push(Command::MoveNote {
                                source: source.join(&path),
                                destination: destination.join(&path),
                                force
                            });
                        }

                        true
                    });

                    return Ok(moves);
                }
            }

            let destination_file_tree = note_file_tree.as_ref().map(|note_file_tree| note_file_tree.find(&destination)).flatten();
            if let (Some(destination_tree), Some(filename)) = (destination_file_tree, source.file_name()) {
                if destination_tree.is_tree() {
                    return Ok(
                        vec![
                            Command::MoveNote {
                                source: source.clone(),
                                destination: destination.join(filename),
                                force
                            }
                        ]
                    );
                }
            }

            Ok(
                vec![
                    Command::MoveNote { source, destination, force }
                ]
            )
        };

        let source_str = source.to_str().unwrap();
        if source_str.contains("*") {
            if let Some(glob_paths) = self.create_glob_paths(&working_dir, note_file_tree.as_ref(), source_str)? {
                let mut commands = Vec::new();
                for source in glob_paths {
                    commands.append(&mut inner(source, destination.clone())?);
                }

                return Ok(commands);
            }
        }

        inner(source, destination)
    }

    fn create_remove_commands(&self,
                              working_dir: PathBuf,
                              path: PathBuf,
                              recursive: bool) -> QueryingResult<Vec<Command>> {
        let note_file_tree = NoteFileTree::from_iter(self.note_metadata_storage_ref()?.notes());

        let inner = |path: PathBuf| {
            let source_file_tree = note_file_tree.as_ref().map(|note_file_tree| note_file_tree.find(&path)).flatten();
            if let Some(note_file_tree) = source_file_tree {
                if note_file_tree.is_tree() && recursive {
                    let mut removes = Vec::new();
                    note_file_tree.walk(|_, parent, name, tree, _| {
                        if tree.is_leaf() {
                            removes.push(Command::RemoveNote { path: path.join(parent.join(name)) });
                        }

                        true
                    });

                    return Ok(removes);
                }
            }

            Ok(
                vec![
                    Command::RemoveNote { path }
                ]
            )
        };

        let path_str = path.to_str().unwrap();
        if path_str.contains("*") {
            if let Some(glob_paths) = self.create_glob_paths(&working_dir, note_file_tree.as_ref(), path_str)? {
                let mut commands = Vec::new();
                for current in glob_paths {
                    commands.append(&mut inner(current)?);
                }

                return Ok(commands);
            }
        }

        inner(path)
    }

    fn create_glob_paths(&self,
                         working_dir: &Path,
                         note_file_tree: Option<&NoteFileTree>,
                         pattern: &str) -> QueryingResult<Option<Vec<PathBuf>>> {
        if let Ok(glob) = Glob::new(pattern) {
            let glob = glob.compile_matcher();

            if let Some(note_file_tree) = note_file_tree.as_ref().map(|tree| tree.find(&working_dir)).flatten() {
                let mut files = Vec::new();
                note_file_tree.walk(|_, parent, name, _, _| {
                    let path = working_dir.join(parent).join(name);
                    if glob.is_match(&path) {
                        files.push(path);
                        false
                    } else {
                        true
                    }
                });

                return Ok(Some(files));
            }
        }

        Ok(None)
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

    fn get_note_content_path(&mut self, path: &Path) -> QueryingResult<PathBuf> {
        self.note_metadata_storage()?;
        let id = self.note_metadata_storage()?
            .get(path)
            .ok_or_else(|| QueryingError::NoteNotFound(path.to_str().unwrap().to_string()))?
            .id;

        Ok(
            NoteMetadataStorage::get_note_storage_path(
                &self.config.repository,
                &id
            ).1
        )
    }

    pub fn clear_cache(&mut self) {
        self.note_metadata_storage = None;
        self.version += 1;
    }

    pub fn has_changed(&self, version: &mut u64) -> bool {
        if *version != self.version {
            *version = self.version;
            true
        } else {
            false
        }
    }

    pub fn working_dir(&mut self) -> AppResult<PathBuf> {
        self.get_path(Path::new("").to_owned())
    }

    pub fn set_working_dir(&mut self, working_dir: &Path) {
        self.working_dir = Some(working_dir.to_owned());
    }

    fn get_path(&mut self, path: PathBuf) -> AppResult<PathBuf> {
        self.note_metadata_storage()?;
        self.note_metadata_storage_ref()?.resolve_path(
            self.working_dir.as_ref(),
            path
        ).map_err(|err| AppError::InvalidPath(err))
    }
}

#[derive(StructOpt)]
#[structopt(about="CLI based notes & snippet application powered by Git.")]
pub struct MainInputCommand {
    /// Don't use current directory as initial working dir
    #[structopt(long="no-working-dir")]
    pub use_non_working_dir: bool,
    #[structopt(subcommand)]
    pub command: Option<InputCommand>
}

impl MainInputCommand {
    pub fn apply(&self, mut config: Config) -> Config {
        if self.use_non_working_dir {
            config.use_working_dir = false;
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
        /// Supported keys: repository, editor, base_dir, sync_default_branch, sync_default_remote
        #[structopt(long)]
        set: Option<String>
    },
    /// Updates the symbolic links
    UpdateSymbolicLinks {

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
        /// The path of the note. Id also work.
        path: PathBuf,
        /// Starts editing the note with content at the given git commit
        #[structopt(long="history")]
        history: Option<String>,
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
        /// The path of the note. Id also work.
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
        /// The path of the note. Id also work.
        path: PathBuf,
        /// Recursively removes all notes in path.
        #[structopt(long, short)]
        recursive: bool
    },
    /// Undo the given commit
    Undo {
        /// The git commit to undo
        commit: String
    },
    /// Runs the code snippet contained in a note.
    #[structopt(name="run")]
    RunSnippet {
        /// The path of the note. Id also work.
        path: PathBuf,
        /// Saves the output of the snippet inside the note.
        #[structopt(long="save")]
        save_output: bool
    },
    /// Converts the given note to a file (like pdf)
    #[structopt(name="convert")]
    ConvertFile {
        /// The path of the note. Id also work.
        path: PathBuf,
        /// The destination of  the path
        destination: PathBuf
    },
    /// Begins a commit. All subsequent operations are done within this commit (interactive mode only).
    Begin {

    },
    /// Commits the started transaction. If no changes have been made, a commit is not created (interactive mode only).
    Commit {

    },
    /// Manages remote git connections
    Remote {
        #[structopt(subcommand)]
        command: InputCommandRemote
    },
    /// Synchronizes the notes with a remote git instance
    #[structopt(name="sync")]
    Synchronize {
        /// The branch to synchronize. If missing, uses default (typically master)
        branch: Option<String>,
        /// The remote to synchronize with. If missing, uses default (typically origin)
        remote: Option<String>,
        /// Don't pull when synchronizing
        #[structopt(long="no-pull")]
        no_pull: bool,
        /// Don't push when synchronizing
        #[structopt(long="no-push")]
        no_push: bool
    },
    /// Prints the content of a note.
    #[structopt(name="cat")]
    PrintContent {
        /// The path of the note. Id also work.
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
        /// The path of the note. Id also work.
        path: PathBuf,
        /// Shows the content at the given git commit
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
    /// Manage resources
    Resource {
        #[structopt(subcommand)]
        command: InputCommandResource
    },
    /// Opens the notes folder in file explorer
    #[structopt(name="open-notes")]
    OpenNotesInFileExplorer {

    },
    /// Lists git commits
    Log {
        /// The number of commits to show. -1 for all.
        #[structopt(default_value="5")]
        count: isize
    },
    /// Shows information about a note
    Info {
        /// The path of the note. Id also work.
        path: PathBuf,
        /// Prints only the file system path.
        #[structopt(long="file-system")]
        only_file_system_path: bool,
    },
    /// Changes the working directory of the (virtual) file system (interactive mode only)
    #[structopt(name="cd")]
    ChangeWorkingDirectory {
        /// The new working directory
        path: PathBuf
    },
    /// Prints the current working directory of the (virtual) file system
    #[structopt(name="pwd")]
    PrintWorkingDirectory {

    },
    /// Runs web editor in stand-alone mode (use web-editor in editor config to use it)
    WebEditor {
        /// The (file system) path to edit
        path: PathBuf,
        /// The part to run the web server at (default: 9000)
        #[structopt(long, default_value="9000")]
        port: u16,
        /// Launches editor in read only mode
        #[structopt(long="read-only")]
        is_read_only: bool
    }
}

#[derive(Debug, StructOpt)]
pub enum InputCommandFinder {
    /// Searches based on tags.
    Tag {
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

impl InputCommandFinder {
    pub fn query(self) -> FindQuery {
        match self {
            InputCommandFinder::Tag { tags } => {
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
        }
    }
}

#[derive(Debug, StructOpt)]
pub enum InputCommandRemote {
    /// Lists the existing remotes
    List {

    },
    /// Adds a new remote
    Add {
        /// The name of the remote
        name: String,
        /// The URL of the remote
        url: String
    },
    /// Removes an existing remote
    Remove {
        /// The name of the remote
        name: String
    }
}

#[derive(Debug, StructOpt)]
pub enum InputCommandResource {
    /// Lists the resources
    List {
        /// The directory to list. Leave empty for all.
        query: Option<PathBuf>,
        /// Prints the absolute path.
        #[structopt(long)]
        print_absolute: bool
    },
    /// Adds a resource to the repository
    Add {
        /// The path of the resource on the local filesystem
        path: PathBuf,
        /// The path of the resource within the repository
        destination: PathBuf
    },
    /// Applies a command on a resource
    Apply {
        /// The command to apply. Path to resource is given as first argument
        command: String,
        /// The resource to apply on
        resource: PathBuf
    },
}

pub type AppResult<T> = Result<T, AppError>;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Failed to open repository: {0}")]
    FailedToOpenRepository(git2::Error),

    #[error("Invalid path: {0}")]
    InvalidPath(String),

    #[error("{0}")]
    Command(CommandError),

    #[error("{0}")]
    Querying(QueryingError),

    #[error("Input error: {0}")]
    Input(String),

    #[error("Failed to change directory: {0}")]
    ChangeDirectory(String),

    #[error("Failed to convert to pdf: {0}")]
    FailedToConvert(String),

    #[error("Remote '{0}' not found")]
    RemoteNotFound(String),

    #[error("{0}")]
    Regex(regex::Error),

    #[error("{0}")]
    Git(git2::Error),

    #[error("{0}")]
    IO(std::io::Error)
}

impl From<CommandError> for AppError {
    fn from(err: CommandError) -> Self {
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

fn open_repository(path: &Path) -> AppResult<git2::Repository> {
    git2::Repository::open(path).map_err(|err| AppError::FailedToOpenRepository(err))
}

fn get_initial_working_dir(config: &Config) -> Option<PathBuf> {
    if !config.use_working_dir {
        return None;
    }

    let base_dir = config.base_dir.as_ref()?;
    let current_dir = std::env::current_dir().ok()?;
    let working_dir = current_dir.strip_prefix(base_dir).ok()?;
    Some(working_dir.to_owned())
}

fn change_working_dir(mut current_working_dir: Option<&Path>, mut path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        if let Ok(base) = path.strip_prefix("/") {
            current_working_dir = None;
            path = base.to_owned();
        }
    }

    let mut current_working_dir = current_working_dir.unwrap_or_else(|| Path::new("")).to_owned();
    for part in path.iter() {
        if part == ".." {
            if let Some(parent) = current_working_dir.parent() {
                current_working_dir = parent.to_owned();
            } else {
                current_working_dir = Path::new("").to_owned();
            }
        } else {
            current_working_dir = current_working_dir.join(part);
        }
    }
    
    current_working_dir
}

#[test]
fn test_change_working_dir1() {
    assert_eq!(
        Path::new("Code"),
        change_working_dir(Some(Path::new("Code/gitnotes-cli")), Path::new("..").to_owned())
    );
}

#[test]
fn test_change_working_dir2() {
    assert_eq!(
        Path::new(""),
        change_working_dir(Some(Path::new("Code/gitnotes-cli")), Path::new("../..").to_owned())
    );
}

#[test]
fn test_change_working_dir3() {
    assert_eq!(
        Path::new("Code/test"),
        change_working_dir(Some(Path::new("Code/gitnotes-cli")), Path::new("../test").to_owned())
    );
}

#[test]
fn test_change_working_dir4() {
    assert_eq!(
        Path::new("Code/gitnotes-cli/test"),
        change_working_dir(Some(Path::new("Code/gitnotes-cli")), Path::new("test").to_owned())
    );
}

#[test]
fn test_change_working_dir5() {
    assert_eq!(
        Path::new("Code/gitnotes-cli/test1/test2"),
        change_working_dir(Some(Path::new("Code/gitnotes-cli")), Path::new("test1/test2").to_owned())
    );
}

#[test]
fn test_change_working_dir6() {
    assert_eq!(
        Path::new("Code"),
        change_working_dir(Some(Path::new("")), Path::new("Code").to_owned())
    );
}

#[test]
fn test_change_working_dir7() {
    assert_eq!(
        Path::new("Code/gitnotes-cli"),
        change_working_dir(Some(Path::new("")), Path::new("Code/gitnotes-cli").to_owned())
    );
}

#[test]
fn test_change_working_dir8() {
    assert_eq!(
        Path::new(""),
        change_working_dir(Some(Path::new("Code/gitnotes-cli")), Path::new("/").to_owned())
    );
}

#[test]
fn test_change_working_dir9() {
    assert_eq!(
        Path::new("projects"),
        change_working_dir(Some(Path::new("Code/gitnotes-cli")), Path::new("/projects").to_owned())
    );
}
