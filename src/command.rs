use std::ops::Deref;
use std::path::{Path, PathBuf};

use chrono::Local;

use thiserror::Error;

use comrak::nodes::NodeValue;
use crate::config::Config;

use crate::model::{NoteId, NoteMetadata, NoteMetadataStorage};
use crate::{editor, markdown};
use crate::app::RepositoryRef;
use crate::helpers::{get_or_insert_with, OrderedSet};
use crate::querying::{GitContentFetcher};
use crate::snippets::{SnippetError, SnippetRunnerManger};

#[derive(Debug)]
pub enum Command {
    AddNote {
        path: PathBuf,
        tags: Vec<String>
    },
    AddNoteWithContent {
        path: PathBuf,
        tags: Vec<String>,
        content: String
    },
    EditNoteContent {
        path: PathBuf,
        history: Option<String>,
        clear_tags: bool,
        add_tags: Vec<String>
    },
    EditNoteSetContent {
        path: PathBuf,
        clear_tags: bool,
        add_tags: Vec<String>,
        content: String
    },
    MoveNote {
        source: PathBuf,
        destination: PathBuf,
        force: bool
    },
    RemoveNote {
        path: PathBuf
    },
    RunSnippet {
        path: PathBuf,
        save_output: bool
    },
    Commit
}

pub type CommandResult<T> = Result<T, CommandError>;

#[derive(Error, Debug)]
pub enum CommandError {
    #[error("Failed to add note: {0}")]
    FailedToAddNote(String),
    #[error("Failed to edit note: {0}")]
    FailedToEditNote(String),
    #[error("Failed to remove note: {0}")]
    FailedToRemoveNote(String),
    #[error("Failed to commit: {0}")]
    FailedToCommit(String),
    #[error("Existing note at destination '{0}', use -f to delete that note before moving")]
    NoteAtDestination(PathBuf),

    #[error("Failed to update metadata: {0}")]
    FailedToUpdateMetadata(String),
    #[error("Note '{0}' not found")]
    NoteNotFound(String),
    #[error("Note '{0}' already exists")]
    NoteAlreadyExists(PathBuf),

    #[error("Failed to run snippet: {0}")]
    Snippet(SnippetError),

    #[error("Internal error: {0}")]
    InternalError(String),

    #[error("Subprocess error: {0}")]
    SubProcess(std::io::Error),
    #[error("{0}")]
    Git(git2::Error),
    #[error(" {0}")]
    IO(std::io::Error)
}

impl From<git2::Error> for CommandError {
    fn from(err: git2::Error) -> Self {
        CommandError::Git(err)
    }
}

impl From<std::io::Error> for CommandError {
    fn from(err: std::io::Error) -> Self {
        CommandError::IO(err)
    }
}

pub struct CommandInterpreter {
    config: Config,

    repository: RepositoryRef,

    note_metadata_storage: Option<NoteMetadataStorage>,
    snippet_runner_manager: SnippetRunnerManger,

    index: Option<git2::Index>,
    commit_message_lines: OrderedSet<String>
}

impl CommandInterpreter {
    pub fn new(config: Config, repository: RepositoryRef) -> CommandResult<CommandInterpreter> {
        let mut snippet_runner_manager = SnippetRunnerManger::default();

        if let Some(snippet_config) = config.snippet.as_ref() {
            snippet_runner_manager.apply_config(snippet_config).map_err(|err| CommandError::Snippet(err))?;
        }

        Ok(
            CommandInterpreter {
                config,

                repository,

                note_metadata_storage: None,
                snippet_runner_manager,

                index: None,
                commit_message_lines: OrderedSet::new()
            }
        )
    }

    pub fn execute(&mut self, commands: Vec<Command>) -> CommandResult<()> {
        use CommandError::*;

        for command in commands.into_iter() {
            match command {
                Command::AddNote { path, tags } => {
                    self.check_if_note_exists(&path)?;

                    let id = NoteId::new();
                    let (relative_note_path, abs_note_path) = self.get_note_storage_path(&id);

                    if !abs_note_path.exists() {
                        std::fs::write(&abs_note_path, "").map_err(|err| FailedToAddNote(err.to_string()))?;
                    }

                    editor::launch(&self.config, &abs_note_path).map_err(|err| FailedToAddNote(err.to_string()))?;

                    self.add_note(id, &relative_note_path, path, tags)?;
                }
                Command::AddNoteWithContent { path, tags, content } => {
                    self.check_if_note_exists(&path)?;

                    let id = NoteId::new();
                    let (relative_note_path, abs_note_path) = self.get_note_storage_path(&id);

                    std::fs::write(&abs_note_path, content).map_err(|err| FailedToAddNote(err.to_string()))?;

                    self.add_note(id, &relative_note_path, path, tags)?;
                }
                Command::EditNoteContent { path, history, clear_tags, add_tags } => {
                    let id = self.get_note_id(&path)?;
                    let (relative_content_path, abs_content_path) = self.get_note_storage_path(&id);
                    let real_path = self.get_note_path(&id)?.to_path_buf();

                    if let Some(history) = history {
                        self.note_metadata_storage()?;

                        let content = GitContentFetcher::new(
                            self.repository.borrow().deref(),
                            self.note_metadata_storage_ref()?
                        ).fetch(&real_path, &history);

                        let content = content.map_err(|err| FailedToEditNote(err.to_string()))?;
                        let content = content.ok_or_else(|| FailedToEditNote(format!("Note '{}' not found at commit '{}'", path.to_str().unwrap(), history)))?;
                        std::fs::write(&abs_content_path, content).map_err(|err| FailedToEditNote(err.to_string()))?;
                    }

                    editor::launch(&self.config, &abs_content_path).map_err(|err| FailedToEditNote(err.to_string()))?;

                    let index = self.index()?;
                    index.add_path(&relative_content_path)?;
                    index.write()?;

                    self.change_note_tags(&id, clear_tags, add_tags)?;
                    let changed = self.try_change_last_updated(&id)?;

                    if changed {
                        self.commit_message_lines.insert(format!("Updated note '{}'.", real_path.to_str().unwrap()));
                    }
                }
                Command::EditNoteSetContent { path, clear_tags, add_tags, content } => {
                    let id = self.get_note_id(&path)?;
                    let (relative_content_path, abs_content_path) = self.get_note_storage_path(&id);

                    std::fs::write(&abs_content_path, content).map_err(|err| FailedToEditNote(err.to_string()))?;

                    let index = self.index()?;
                    index.add_path(&relative_content_path)?;
                    index.write()?;

                    self.change_note_tags(&id, clear_tags, add_tags)?;
                    self.try_change_last_updated(&id)?;

                    let real_path = self.get_note_path(&id)?.to_str().unwrap().to_owned();
                    self.commit_message_lines.insert(format!("Updated note '{}'.", real_path));
                }
                Command::MoveNote { source, destination, force } => {
                    let id = self.get_note_id(&source)?;
                    let real_source_path = self.get_note_path(&id)?.to_str().unwrap().to_owned();

                    let destination_exist = self.get_note_id(&destination).is_ok();
                    if destination_exist {
                        if force {
                            self.remove_note(&destination)?;
                        } else {
                            return Err(NoteAtDestination(destination))?;
                        }
                    }

                    self.change_note_metadata(&id, |note_metadata| {
                        note_metadata.path = destination.clone();
                        true
                    })?;

                    self.try_change_last_updated(&id)?;

                    self.commit_message_lines.insert(format!("Moved note from '{}' to '{}'.", real_source_path, destination.to_str().unwrap()));
                }
                Command::RemoveNote { path } => {
                    self.remove_note(&path)?;
                }
                Command::RunSnippet { path, save_output } => {
                    let id = self.get_note_id(&path)?;
                    let (relative_note_path, abs_note_path) = self.get_note_storage_path(&id);

                    let content = std::fs::read_to_string(&abs_note_path)?;

                    let arena = markdown::storage();
                    let root = markdown::parse(&arena, &content);

                    markdown::visit_code_blocks::<CommandError, _>(
                        &root,
                        |current_node| {
                            if let NodeValue::CodeBlock(ref block) = current_node.data.borrow().value {
                                let snippet_result = self.snippet_runner_manager.run(
                                    &block.info,
                                    &block.literal
                                );

                                let output_stdout = match snippet_result {
                                    Ok(output_stdout) => {
                                        print!("{}", output_stdout);
                                        output_stdout
                                    }
                                    Err(SnippetError::Execution { status, output }) => {
                                        print!("{}", output);
                                        return Err(Snippet(SnippetError::Execution { status, output }));
                                    }
                                    Err(err) => {
                                        return Err(Snippet(err));
                                    }
                                };

                                let mut create_output_node = true;
                                if let Some(next_node) = current_node.next_sibling() {
                                    match next_node.data.borrow_mut().value {
                                        NodeValue::CodeBlock(ref mut output_block) => {
                                            if output_block.info == "output" {
                                                output_block.literal = output_stdout.clone();
                                                create_output_node = false;
                                            }
                                        }
                                        _ => {}
                                    }
                                }

                                if create_output_node {
                                    current_node.insert_after(markdown::create_output_code_block(&arena, output_stdout));
                                }
                            }

                            Ok(())
                        },
                        true,
                        false
                    )?;

                    if save_output {
                        std::fs::write(abs_note_path, markdown::ast_to_string(&root)?)?;

                        let index = self.index()?;
                        index.add_path(&relative_note_path)?;
                        index.write()?;

                        self.try_change_last_updated(&id)?;

                        let real_path = self.get_note_path(&id)?.to_str().unwrap().to_owned();
                        self.commit_message_lines.insert(format!("Saved run output for note '{}'.", real_path));
                    }
                }
                Command::Commit => {
                    let new_tree = self.index()?.write_tree()?;
                    let repository = self.repository.borrow();
                    let new_tree = repository.find_tree(new_tree)?;

                    // Handle that this might be the first commit
                    let create = match CommandInterpreter::get_git_head(repository.deref()) {
                        Ok((head_commit, head_tree)) => {
                            if CommandInterpreter::has_git_diff(repository.deref(), &head_tree, &new_tree)? {
                                Some(Some(head_commit))
                            } else {
                                None
                            }
                        }
                        Err(_) => {
                            Some(None)
                        }
                    };

                    if let Some(head_commit) = create {
                        let head_commit = head_commit.as_ref().map(|h| vec![h]).unwrap_or_else(|| vec![]);

                        let signature = git2::Signature::now(&self.config.user_name_and_email.0, &self.config.user_name_and_email.1)?;
                        let commit_message = std::mem::take(&mut self.commit_message_lines).into_iter().collect::<Vec<_>>().join("\n");
                        self.repository.borrow().commit(
                            Some("HEAD"),
                            &signature,
                            &signature,
                            &commit_message,
                            &new_tree,
                            &head_commit
                        ).map_err(|err| FailedToCommit(err.to_string()))?;
                        println!("Created commit with message:\n{}", commit_message);

                        self.index = None;
                        self.note_metadata_storage = None;
                    }
                }
            }
        }

        Ok(())
    }

    pub fn new_commit(&mut self) -> CommandResult<()> {
        self.index = None;
        self.commit_message_lines.clear();
        Ok(())
    }

    fn add_note(&mut self,
                id: NoteId, relative_path: &Path,
                path: PathBuf, mut tags: Vec<String>) -> CommandResult<()> {
        use CommandError::*;
        use crate::tags;

        if tags.is_empty() {
            let (_, abs_content_path) = self.get_note_storage_path(&id);
            let content = std::fs::read_to_string(abs_content_path)?;
            tags = tags::automatic(&content);
        }

        let (relative_metadata_path, abs_metadata_path) = self.get_note_metadata_path(&id);
        let metadata = NoteMetadata::new(id, path.to_owned(), tags);
        metadata.save(&abs_metadata_path).map_err(|err| FailedToAddNote(err.to_string()))?;

        let index = self.index()?;
        index.add_path(&relative_path)?;
        index.add_path(&relative_metadata_path)?;
        index.write()?;

        self.commit_message_lines.insert(format!(
            "Added note '{}' (id: {}) using tags: {}.",
            path.to_str().unwrap(),
            id,
            metadata.tags.join(", ")
        ));

        Ok(())
    }

    fn remove_note(&mut self, path: &Path) -> CommandResult<()> {
        use CommandError::*;

        let id = self.get_note_id(path)?;
        let real_path = self.get_note_path(&id)?.to_str().unwrap().to_owned();

        let (relative_content_path, abs_content_path) = self.get_note_storage_path(&id);
        let (relative_metadata_path, abs_metadata_path) = self.get_note_metadata_path(&id);

        std::fs::remove_file(abs_content_path).map_err(|err| FailedToRemoveNote(err.to_string()))?;
        std::fs::remove_file(abs_metadata_path).map_err(|err| FailedToRemoveNote(err.to_string()))?;

        let index = self.index()?;
        index.remove_path(&relative_content_path)?;
        index.remove_path(&relative_metadata_path)?;
        index.write()?;

        self.commit_message_lines.insert(format!("Deleted note '{}'.", real_path));

        Ok(())
    }

    fn try_change_last_updated(&mut self, id: &NoteId) -> CommandResult<bool> {
        if self.has_git_changes()? {
            let (relative_metadata_path, abs_metadata_path) = self.get_note_metadata_path(&id);
            let note_metadata = self.get_note_metadata_mut(&id)?;
            note_metadata.last_updated = Local::now();
            note_metadata.save(&abs_metadata_path)?;

            let index = self.index()?;
            index.add_path(&relative_metadata_path)?;
            index.write()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn change_note_tags(&mut self, id: &NoteId, clear_tags: bool, mut add_tags: Vec<String>) -> CommandResult<()> {
        self.change_note_metadata(id, move |note_metadata| {
            let mut changed_tags = false;
            if clear_tags {
                note_metadata.tags.clear();
                changed_tags = true;
            }

            if !add_tags.is_empty() {
                note_metadata.tags.append(&mut add_tags);
                changed_tags = true;
            }

            changed_tags
        })?;

        Ok(())
    }

    fn change_note_metadata<F: FnMut(&mut NoteMetadata) -> bool>(&mut self, id: &NoteId, mut apply: F) -> CommandResult<()> {
        let mut internal = || -> CommandResult<()> {
            let (relative_metadata_path, abs_metadata_path) = self.get_note_metadata_path(&id);
            let note_metadata = self.get_note_metadata_mut(&id)?;

            if apply(note_metadata) {
                note_metadata.save(&abs_metadata_path)?;

                let index = self.index()?;
                index.add_path(&relative_metadata_path)?;
                index.write()?;
            }

            Ok(())
        };

        internal().map_err(|err| CommandError::FailedToUpdateMetadata(err.to_string()))
    }

    fn has_git_changes(&mut self) -> CommandResult<bool> {
        let new_tree = self.index()?.write_tree()?;
        let repository = self.repository.borrow();
        let new_tree = repository.find_tree(new_tree)?;

        let (_, head_tree) = CommandInterpreter::get_git_head(repository.deref())?;
        CommandInterpreter::has_git_diff(repository.deref(), &head_tree, &new_tree)
    }

    fn get_git_head(repository: &git2::Repository) -> CommandResult<(git2::Commit, git2::Tree)> {
        let head = repository.head()?;
        let head_commit = head.peel(git2::ObjectType::Commit)?;
        let head_commit = head_commit.as_commit().unwrap().clone();

        let head_tree = head.peel(git2::ObjectType::Tree)?;
        let head_tree = head_tree.as_tree().unwrap().clone();

        Ok((head_commit, head_tree))
    }

    fn has_git_diff(repository: &git2::Repository, head_tree: &git2::Tree, new_tree: &git2::Tree) -> CommandResult<bool> {
        let diff = repository.diff_tree_to_tree(Some(&new_tree), Some(&head_tree), None)?;
        Ok(diff.stats()?.files_changed() > 0)
    }

    fn get_note_storage_path(&self, id: &NoteId) -> (PathBuf, PathBuf) {
        NoteMetadataStorage::get_note_storage_path(&self.config.repository, id)
    }

    fn get_note_metadata_path(&self, id: &NoteId) -> (PathBuf, PathBuf) {
        NoteMetadataStorage::get_note_metadata_path(&self.config.repository, id)
    }

    fn get_note_id(&mut self, path: &Path) -> CommandResult<NoteId> {
        self.note_metadata_storage()?
            .get_id(path)
            .ok_or_else(|| CommandError::NoteNotFound(path.to_str().unwrap().to_owned()))
    }

    fn get_note_path(&mut self, id: &NoteId) -> CommandResult<&Path> {
        self.note_metadata_storage()?
            .get_by_id(id)
            .map(|note| note.path.as_path())
            .ok_or_else(|| CommandError::NoteNotFound(id.to_string()))
    }

    fn get_note_metadata_mut(&mut self, id: &NoteId) -> CommandResult<&mut NoteMetadata> {
        self.note_metadata_storage_mut()?
            .get_by_id_mut(id)
            .ok_or_else(|| CommandError::NoteNotFound(id.to_string()))
    }

    fn check_if_note_exists(&mut self, path: &Path) -> CommandResult<()> {
        if self.note_metadata_storage()?.contains_path(path) {
            Err(CommandError::NoteAlreadyExists(path.to_owned()))
        } else {
            Ok(())
        }
    }

    fn note_metadata_storage(&mut self) -> CommandResult<&NoteMetadataStorage> {
        self.note_metadata_storage_mut().map(|x| &*x)
    }

    fn note_metadata_storage_ref(&self) -> CommandResult<&NoteMetadataStorage> {
        self.note_metadata_storage
            .as_ref()
            .ok_or_else(|| CommandError::InternalError("note_metadata_storage not created".to_owned()))
    }

    fn note_metadata_storage_mut(&mut self) -> CommandResult<&mut NoteMetadataStorage> {
        get_or_insert_with(
            &mut self.note_metadata_storage,
            || Ok(NoteMetadataStorage::from_dir(&self.config.repository)?)
        )
    }

    fn index(&mut self) -> CommandResult<&mut git2::Index> {
        CommandInterpreter::get_index(self.repository.borrow().deref(), &mut self.index)
    }

    fn get_index<'a>(repository: &git2::Repository,
                     index: &'a mut Option<git2::Index>) -> CommandResult<&'a mut git2::Index> {
        get_or_insert_with(index, || Ok(repository.index()?))
    }
}