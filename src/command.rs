use std::ffi::{OsStr};
use std::ops::Deref;
use std::path::{Path, PathBuf};

use chrono::Local;
use comrak::Arena;
use thiserror::Error;

use comrak::nodes::{AstNode, NodeValue};

use crate::config::Config;
use crate::model::{NOTE_CONTENT_EXT, NoteId, NoteMetadata, NoteMetadataStorage, NOTES_DIR, RESOURCES_DIR};
use crate::{editor, markdown, tags};
use crate::app::{RepositoryRef};
use crate::editor::EditorOutput;
use crate::helpers::{get_or_insert_with, OrderedSet};
use crate::querying::{GitContentFetcher};
use crate::snippets::{SnippetError, SnippetRunnerManger};
use crate::web_editor::AccessMode;

#[derive(Debug)]
pub enum Command {
    UpdateSymbolicLinks {

    },
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
    UndoCommit {
        commit: String
    },
    RunSnippet {
        path: PathBuf,
        save_output: bool
    },
    AddResource {
        path: PathBuf,
        destination: PathBuf
    },
    Commit
}

pub type LaunchEditorFn = Box<dyn Fn(&Config, &Path, &Path) -> CommandResult<EditorOutput>>;
pub struct CommandInterpreter {
    config: Config,

    launch_editor: LaunchEditorFn,

    repository: RepositoryRef,

    note_metadata_storage: Option<NoteMetadataStorage>,
    snippet_runner_manager: SnippetRunnerManger,

    index: Option<git2::Index>,
    commit_message_lines: OrderedSet<String>,
    changed_files: Vec<PathBuf>
}

impl CommandInterpreter {
    pub fn new(config: Config, repository: RepositoryRef) -> CommandResult<CommandInterpreter> {
        CommandInterpreter::with_launch_editor(
            config,
            repository.clone(),
            Box::new(|config, path, note_path| editor::launch(config, path, Some(note_path), AccessMode::default()))
        )
    }

    pub fn with_launch_editor(config: Config, repository: RepositoryRef, launch_editor: LaunchEditorFn) -> CommandResult<CommandInterpreter> {
        let snippet_runner_manager = SnippetRunnerManger::from_config(
            config.snippet.as_ref()
        ).map_err(|err| CommandError::Snippet(err))?;

        Ok(
            CommandInterpreter {
                config,

                launch_editor,

                repository,

                note_metadata_storage: None,
                snippet_runner_manager,

                index: None,
                commit_message_lines: OrderedSet::new(),
                changed_files: Vec::new()
            }
        )
    }

    pub fn execute(&mut self, commands: Vec<Command>) -> CommandResult<()> {
        use CommandError::*;

        for command in commands.into_iter() {
            match command {
                Command::UpdateSymbolicLinks { } => {
                    self.note_metadata_storage()?;
                    let note_metadata_storage = self.note_metadata_storage_ref()?;

                    clear_note_symbolic_links(&self.config.repository)?;

                    for note in note_metadata_storage.notes() {
                        create_note_symbolic_link(&self.config.repository, note)?;
                    }
                }
                Command::AddNote { path, tags } => {
                    self.check_if_note_exists(&path)?;

                    let id = NoteId::new();
                    let (relative_content_path, abs_content_path) = self.get_note_storage_path(&id);

                    if !abs_content_path.exists() {
                        std::fs::write(&abs_content_path, "").map_err(|err| FailedToAddNote(err.to_string()))?;
                    }

                    let output = (self.launch_editor)(
                        &self.config,
                        &abs_content_path,
                        &path
                    ).map_err(|err| FailedToAddNote(err.to_string()))?;

                    self.add_note(id, &relative_content_path, path, tags)?;
                    self.add_resources_from_editor_output(output)?;
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
                    let note_path = self.get_note_path(&id)?.to_owned();

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

                    let output = (self.launch_editor)(
                        &self.config,
                        &abs_content_path,
                        &note_path,
                    ).map_err(|err| FailedToEditNote(err.to_string()))?;

                    self.edited_file(relative_content_path)?;

                    self.change_note_tags(&id, clear_tags, add_tags)?;
                    let changed = self.try_change_last_updated(&id)?;

                    if changed {
                        self.commit_message_lines.insert(format!("Updated note '{}'.", real_path.to_str().unwrap()));
                    }

                    self.add_resources_from_editor_output(output)?;
                }
                Command::EditNoteSetContent { path, clear_tags, add_tags, content } => {
                    let id = self.get_note_id(&path)?;
                    let (relative_content_path, abs_content_path) = self.get_note_storage_path(&id);

                    std::fs::write(&abs_content_path, content).map_err(|err| FailedToEditNote(err.to_string()))?;

                    self.edited_file(relative_content_path)?;

                    self.change_note_tags(&id, clear_tags, add_tags)?;
                    self.try_change_last_updated(&id)?;

                    let real_path = self.get_note_path(&id)?.to_str().unwrap().to_owned();
                    self.commit_message_lines.insert(format!("Updated note '{}'.", real_path));
                }
                Command::MoveNote { source, destination, force } => {
                    let id = self.get_note_id(&source)?;
                    let real_source_path = self.get_note_path(&id)?.to_str().unwrap().to_owned();

                    self.note_metadata_storage_mut()?;
                    let note_symbolic_link = get_note_symbolic_link(&self.config.repository, self.get_note_metadata(&id)?)?;

                    let destination_exist = self.get_note_id(&destination).is_ok();
                    if destination_exist {
                        if force {
                            self.remove_note(&destination)?;
                        } else {
                            return Err(NoteExistsAtDestination(destination))?;
                        }
                    }

                    self.change_note_metadata(&id, |note_metadata| {
                        note_metadata.path = destination.clone();
                        true
                    })?;

                    self.try_change_last_updated(&id)?;

                    let _ = std::fs::remove_file(&note_symbolic_link);
                    create_note_symbolic_link(&self.config.repository, self.get_note_metadata(&id)?)?;

                    self.commit_message_lines.insert(
                        format!("Moved note from '{}' to '{}'.", real_source_path, destination.to_str().unwrap())
                    );
                }
                Command::RemoveNote { path } => {
                    self.remove_note(&path)?;
                }
                Command::UndoCommit { commit } => {
                    let git_commit_id = {
                        let repository = self.repository.borrow_mut();
                        let git_commit = repository.revparse_single(&commit)?;
                        let git_commit = git_commit.as_commit().ok_or_else(|| CommitNotFound(commit.clone()))?;
                        let git_commit_id = git_commit.as_object().short_id().unwrap().as_str().unwrap().to_owned();

                        repository.revert(&git_commit, None).map_err(|err| FailedToUndo(err.to_string()))?;
                        repository.cleanup_state()?;

                        git_commit_id
                    };

                    self.commit_message_lines.insert(format!("Undo commit '{}'.", git_commit_id));
                },
                Command::RunSnippet { path, save_output } => {
                    let id = self.get_note_id(&path)?;
                    let (relative_note_path, abs_note_path) = self.get_note_storage_path(&id);

                    let content = std::fs::read_to_string(&abs_note_path)?;

                    let arena = markdown::storage();
                    let root = run_snippet(&self.snippet_runner_manager, &arena, &content, |text| print!("{}", text))?;

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
                Command::AddResource { path, destination } => {
                    if path.exists() {
                        let destination_resource_path = Path::new(RESOURCES_DIR).join(&destination);
                        let destination_path = destination_resource_path.join(&destination);
                        if let Some(destination_parent) = destination_path.parent() {
                            std::fs::create_dir_all(destination_parent)?;
                        }

                        std::fs::copy(&path, &destination_path)?;

                        let index = self.index()?;
                        index.add_path(&destination_resource_path)?;
                        index.write()?;

                        self.commit_message_lines.insert(format!(
                            "Added resource '{}' (from {}).",
                            destination.to_str().unwrap_or("N/A"),
                            path.to_str().unwrap_or("N/A")
                        ));
                    } else {
                        return Err(ResourceNotFound(path.to_str().unwrap_or("N/A").to_owned()));
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
                        println!("Created commit with message:");
                        for line in commit_message.lines() {
                            println!("\t{}", line);
                        }

                        self.index = None;
                        self.note_metadata_storage = None;
                        self.changed_files.clear();
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

    pub fn reset(&mut self) -> CommandResult<()> {
        let repository = self.repository.borrow_mut();
        let head = repository.head()?;
        let head_commit = head.peel(git2::ObjectType::Commit)?;

        let mut checkout_builder = git2::build::CheckoutBuilder::new();
        for path in &self.changed_files {
            checkout_builder.path(path.to_str().unwrap());
        }

        repository.reset(
            &head_commit,
            git2::ResetType::Hard,
            Some(&mut checkout_builder)
        )?;

        self.changed_files.clear();
        self.commit_message_lines.clear();

        Ok(())
    }

    fn add_note(&mut self,
                id: NoteId, relative_path: &Path,
                path: PathBuf, mut tags: Vec<String>) -> CommandResult<()> {
        use CommandError::*;

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

        create_note_symbolic_link(&self.config.repository, &metadata)?;

        let tags_str = if !metadata.tags.is_empty() {
            format!(" using tags: {}", metadata.tags.join(", "))
        } else {
            String::new()
        };

        self.commit_message_lines.insert(format!(
            "Added note '{}' (id: {}){}.",
            path.to_str().unwrap(),
            id,
            tags_str
        ));

        Ok(())
    }

    fn remove_note(&mut self, path: &Path) -> CommandResult<()> {
        use CommandError::*;

        let id = self.get_note_id(path)?;
        let real_path = self.get_note_path(&id)?.to_str().unwrap().to_owned();
        let note_symbolic_link = get_note_symbolic_link(&self.config.repository, self.get_note_metadata(&id)?)?;

        let (relative_content_path, abs_content_path) = self.get_note_storage_path(&id);
        let (relative_metadata_path, abs_metadata_path) = self.get_note_metadata_path(&id);

        std::fs::remove_file(abs_content_path).map_err(|err| FailedToRemoveNote(err.to_string()))?;
        std::fs::remove_file(abs_metadata_path).map_err(|err| FailedToRemoveNote(err.to_string()))?;

        let index = self.index()?;
        index.remove_path(&relative_content_path)?;
        index.remove_path(&relative_metadata_path)?;
        index.write()?;

        let _ = std::fs::remove_file(note_symbolic_link);

        self.commit_message_lines.insert(format!("Deleted note '{}'.", real_path));
        self.changed_files.push(relative_metadata_path);

        Ok(())
    }

    fn edited_file(&mut self, path: PathBuf) -> CommandResult<()> {
        let index = self.index()?;
        index.add_path(&path)?;
        index.write()?;
        self.changed_files.push(path);
        Ok(())
    }

    fn add_resources_from_editor_output(&mut self, output: EditorOutput) -> CommandResult<()> {
        for path in output.added_resources.iter() {
            self.index()?.add_path(&Path::new(RESOURCES_DIR).join(path))?;

            self.commit_message_lines.insert(format!(
                "Added resource '{}'",
                path.to_str().unwrap_or("N/A")
            ));
        }

        if !output.added_resources.is_empty() {
            self.index()?.write()?;
        }

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

            self.changed_files.push(relative_metadata_path);
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

                self.changed_files.push(relative_metadata_path);
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

    fn get_note_metadata(&self, id: &NoteId) -> CommandResult<&NoteMetadata> {
        self.note_metadata_storage_ref()?
            .get_by_id(id)
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
            || Ok(NoteMetadataStorage::from_dir_with_config(&self.config)?)
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
    #[error("Failed to undo commit: {0}")]
    FailedToUndo(String),

    #[error("Failed to update metadata: {0}")]
    FailedToUpdateMetadata(String),
    #[error("Note '{0}' not found")]
    NoteNotFound(String),
    #[error("Note '{0}' already exists")]
    NoteAlreadyExists(PathBuf),
    #[error("Existing note at destination '{0}', use -f to delete that note before moving")]
    NoteExistsAtDestination(PathBuf),

    #[error("Commit {0} not found")]
    CommitNotFound(String),

    #[error("Failed to run snippet: {0}")]
    Snippet(SnippetError),

    #[error("Resource not found: {0}")]
    ResourceNotFound(String),

    #[error("Branch '{0}' not found")]
    BranchNotFound(String),

    #[error("Internal error: {0}")]
    InternalError(String),

    #[error("{0}")]
    SubProcess(std::io::Error),
    #[error("{0}")]
    Git(git2::Error),
    #[error("{0}")]
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

pub fn run_snippet<'a, F: FnMut(&str)>(snippet_runner_manager: &SnippetRunnerManger,
                                       arena: &'a Arena<AstNode<'a>>,
                                       content: &str,
                                       mut do_print: F) -> CommandResult<&'a AstNode<'a>> {
    let root = markdown::parse(&arena, content);

    markdown::visit_code_blocks::<CommandError, _>(
        &root,
        |current_node| {
            if let NodeValue::CodeBlock(ref block) = current_node.data.borrow().value {
                let snippet_result = snippet_runner_manager.run(
                    &block.info,
                    &block.literal
                );

                let output_stdout = match snippet_result {
                    Ok(output_stdout) => {
                        do_print(&output_stdout);
                        output_stdout
                    }
                    Err(SnippetError::Execution { status, output }) => {
                        do_print(&output);
                        return Err(CommandError::Snippet(SnippetError::Execution { status, output }));
                    }
                    Err(err) => {
                        return Err(CommandError::Snippet(err));
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

    Ok(root)
}

fn create_note_symbolic_link(repository: &Path, note: &NoteMetadata) -> CommandResult<()> {
    let (relative_note_path, _) = NoteMetadataStorage::get_note_storage_path(repository, &note.id);
    let symbolic_link_path = get_note_symbolic_link(repository, note)?;

    let path_length = symbolic_link_path.strip_prefix(repository).unwrap()
        .components().count();
    let mut relative_target_path = PathBuf::from_iter((0..path_length - 1).map(|_| OsStr::new("..")));
    relative_target_path.push(&relative_note_path);

    if let Some(parent) = symbolic_link_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let _ = std::fs::remove_file(&symbolic_link_path);
    std::os::unix::fs::symlink(&relative_target_path, &symbolic_link_path)?;

    Ok(())
}

fn clear_note_symbolic_links(repository: &Path) -> CommandResult<()> {
    for entry in std::fs::read_dir(repository)? {
        let entry = entry?;
        if let Some(file_name) = entry.file_name().to_str() {
            if !(file_name == NOTES_DIR || file_name == RESOURCES_DIR || file_name.starts_with(".")) {
                std::fs::remove_dir_all(entry.path())?;
            }
        }
    }

    Ok(())
}

fn get_note_symbolic_link(repository: &Path, note: &NoteMetadata) -> CommandResult<PathBuf> {
    let path = repository.join(&note.path);
    let mut components = path.components().map(|c| c.as_os_str()).collect::<Vec<_>>();
    let mut last_component = components.last().unwrap().to_os_string();
    last_component.push(&format!(".{}", NOTE_CONTENT_EXT));
    *components.last_mut().unwrap() = last_component.as_os_str();
    Ok(PathBuf::from_iter(components))
}