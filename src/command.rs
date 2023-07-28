use std::path::{Path, PathBuf};

use chrono::Local;

use thiserror::Error;

use comrak::nodes::NodeValue;
use crate::config::Config;

use crate::model::{NoteId, NoteMetadata, NoteMetadataStorage};
use crate::{editor, markdown};
use crate::snippets::{SnipperRunnerManger, SnippetError};

#[derive(Debug)]
pub enum Command {
    AddNote {
        path: PathBuf,
        tags: Vec<String>
    },
    EditNoteContent {
        path: PathBuf,
        clear_tags: bool,
        add_tags: Vec<String>
    },
    AddNoteWithContent {
        path: PathBuf,
        tags: Vec<String>,
        content: String
    },
    EditNoteSetContent {
        path: PathBuf,
        content: String
    },
    MoveNote {
        source: PathBuf,
        destination: PathBuf,
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

pub type CommandInterpreterResult<T> = Result<T, CommandInterpreterError>;

#[derive(Error, Debug)]
pub enum CommandInterpreterError {
    #[error("Failed to open repository: {0}")]
    FailedToOpenRepository(git2::Error),
    #[error("Failed to add note: {0}")]
    FailedToAddNote(String),
    #[error("Failed to edit note: {0}")]
    FailedToEditNote(String),
    #[error("Failed to remove note: {0}")]
    FailedToRemoveNote(String),
    #[error("Failed to commit: {0}")]
    FailedToCommit(String),

    #[error("Failed to update metadata: {0}")]
    FailedToUpdateMetadata(String),
    #[error("Note '{0}' not found")]
    NoteNotFound(String),
    #[error("Note '{0}' already exists")]
    NoteAlreadyExists(PathBuf),

    #[error("Failed to run snippet: {0}")]
    Snippet(SnippetError),

    #[error("Subprocess error: {0}")]
    SubProcess(std::io::Error),
    #[error("Git error: {0}")]
    Git(git2::Error),
    #[error("I/O error: {0}")]
    IO(std::io::Error)
}

impl From<git2::Error> for CommandInterpreterError {
    fn from(err: git2::Error) -> Self {
        CommandInterpreterError::Git(err)
    }
}

impl From<std::io::Error> for CommandInterpreterError {
    fn from(err: std::io::Error) -> Self {
        CommandInterpreterError::IO(err)
    }
}

pub struct CommandInterpreter {
    config: Config,

    repository: git2::Repository,

    note_metadata_storage: Option<NoteMetadataStorage>,
    snippet_runner_manager: SnipperRunnerManger,

    index: Option<git2::Index>,
    commit_message_lines: Vec<String>
}

impl CommandInterpreter {
    pub fn new(config: Config) -> CommandInterpreterResult<CommandInterpreter> {
        let repository = git2::Repository::open(&config.repository).map_err(|err| CommandInterpreterError::FailedToOpenRepository(err))?;

        Ok(
            CommandInterpreter {
                config,

                repository,

                note_metadata_storage: None,
                snippet_runner_manager: SnipperRunnerManger::default(),

                index: None,
                commit_message_lines: Vec::new()
            }
        )
    }

    pub fn execute(&mut self, commands: Vec<Command>) -> CommandInterpreterResult<()> {
        use CommandInterpreterError::*;

        for command in commands.into_iter() {
            match command {
                Command::AddNote { path, tags } => {
                    self.check_if_note_exists(&path)?;

                    let id = NoteId::new();
                    let (relative_note_path, abs_note_path) = self.get_note_storage_path(&id);

                    editor::launch(&self.config, &abs_note_path).map_err(|err| FailedToAddNote(err.to_string()))?;

                    self.add_note(id, &relative_note_path, path, tags)?;
                }
                Command::EditNoteContent { path, clear_tags, mut add_tags } => {
                    let id = self.get_note_id(&path)?;
                    let (relative_content_path, abs_content_path) = self.get_note_storage_path(&id);

                    editor::launch(&self.config, &abs_content_path).map_err(|err| FailedToEditNote(err.to_string()))?;

                    let index = self.index()?;
                    index.add_path(&relative_content_path)?;
                    index.write()?;

                    self.change_note_metadata(&id, move |note_metadata| {
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

                    self.try_change_last_updated(&id)?;

                    let real_path = self.get_note_path(&id)?.to_str().unwrap().to_owned();
                    self.commit_message_lines.push(format!("Updated note '{}'.", real_path));
                }
                Command::AddNoteWithContent { path, tags, content } => {
                    self.check_if_note_exists(&path)?;

                    let id = NoteId::new();
                    let (relative_note_path, abs_note_path) = self.get_note_storage_path(&id);

                    std::fs::write(&abs_note_path, content).map_err(|err| FailedToAddNote(err.to_string()))?;

                    self.add_note(id, &relative_note_path, path, tags)?;
                }
                Command::EditNoteSetContent { path, content } => {
                    let id = self.get_note_id(&path)?;
                    let (relative_content_path, abs_content_path) = self.get_note_storage_path(&id);

                    std::fs::write(&abs_content_path, content).map_err(|err| FailedToEditNote(err.to_string()))?;

                    let index = self.index()?;
                    index.add_path(&relative_content_path)?;
                    index.write()?;

                    self.try_change_last_updated(&id)?;

                    let real_path = self.get_note_path(&id)?.to_str().unwrap().to_owned();
                    self.commit_message_lines.push(format!("Updated note '{}'.", real_path));
                }
                Command::MoveNote { source, destination } => {
                    let id = self.get_note_id(&source)?;
                    let real_source_path = self.get_note_path(&id)?.to_str().unwrap().to_owned();

                    self.change_note_metadata(&id, |note_metadata| {
                        note_metadata.path = destination.clone();
                        true
                    })?;

                    self.try_change_last_updated(&id)?;

                    self.commit_message_lines.push(format!("Moved note from '{}' to '{}'.", real_source_path, destination.to_str().unwrap()));
                }
                Command::RemoveNote { path } => {
                    let id = self.get_note_id(&path)?;
                    let real_path = self.get_note_path(&id)?.to_str().unwrap().to_owned();

                    let (relative_content_path, abs_content_path) = self.get_note_storage_path(&id);
                    let (relative_metadata_path, abs_metadata_path) = self.get_note_metadata_path(&id);

                    std::fs::remove_file(abs_content_path).map_err(|err| FailedToRemoveNote(err.to_string()))?;
                    std::fs::remove_file(abs_metadata_path).map_err(|err| FailedToRemoveNote(err.to_string()))?;

                    let index = self.index()?;
                    index.remove_path(&relative_content_path)?;
                    index.remove_path(&relative_metadata_path)?;
                    index.write()?;

                    self.commit_message_lines.push(format!("Deleted note '{}'.", real_path));
                }
                Command::RunSnippet { path, save_output } => {
                    let id = self.get_note_id(&path)?;
                    let (relative_note_path, abs_note_path) = self.get_note_storage_path(&id);

                    let content = std::fs::read_to_string(&abs_note_path)?;

                    let arena = markdown::storage();
                    let root = markdown::parse(&arena, &content);

                    markdown::visit_code_blocks::<CommandInterpreterError, _>(
                        &root,
                        |current_node| {
                            if let NodeValue::CodeBlock(ref block) = current_node.data.borrow().value {
                                let output_stdout = self.snippet_runner_manager.run(
                                    &block.info,
                                    &block.literal
                                ).map_err(|err| Snippet(err))?;
                                println!("{}", output_stdout);

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
                        self.commit_message_lines.push(format!("Saved run output for note '{}'.", real_path));
                    }
                }
                Command::Commit => {
                    let new_tree = self.index()?.write_tree()?;
                    let new_tree = self.repository.find_tree(new_tree)?;

                    // Handle that this might be the first commit
                    let create = match CommandInterpreter::get_git_head(&self.repository) {
                        Ok((head_commit, head_tree)) => {
                            if CommandInterpreter::has_git_diff(&self.repository, &head_tree, &new_tree)? {
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
                        let commit_message = self.commit_message_lines.join("\n");
                        self.repository.commit(
                            Some("HEAD"),
                            &signature,
                            &signature,
                            &commit_message,
                            &new_tree,
                            &head_commit
                        ).map_err(|err| FailedToCommit(err.to_string()))?;
                        println!("Created commit with message:\n{}", commit_message);

                        self.commit_message_lines.clear();
                        self.index = None;
                        self.note_metadata_storage = None;
                    }
                }
            }
        }

        Ok(())
    }

    fn add_note(&mut self,
                id: NoteId, relative_path: &Path,
                path: PathBuf, tags: Vec<String>) -> CommandInterpreterResult<()> {
        use CommandInterpreterError::*;

        let (relative_metadata_path, abs_metadata_path) = self.get_note_metadata_path(&id);
        let metadata = NoteMetadata::new(id, path.to_owned(), tags);
        metadata.save(&abs_metadata_path).map_err(|err| FailedToAddNote(err.to_string()))?;

        let index = self.index()?;
        index.add_path(&relative_path)?;
        index.add_path(&relative_metadata_path)?;
        index.write()?;

        self.commit_message_lines.push(format!("Added note '{}'.", path.to_str().unwrap()));

        Ok(())
    }

    fn try_change_last_updated(&mut self, id: &NoteId) -> CommandInterpreterResult<()> {
        if self.has_git_changes()? {
            let (relative_metadata_path, abs_metadata_path) = self.get_note_metadata_path(&id);
            let note_metadata = self.get_note_metadata_mut(&id)?;
            note_metadata.last_updated = Local::now();
            note_metadata.save(&abs_metadata_path)?;

            let index = self.index()?;
            index.add_path(&relative_metadata_path)?;
            index.write()?;
        }

        Ok(())
    }

    fn change_note_metadata<F: FnMut(&mut NoteMetadata) -> bool>(&mut self, id: &NoteId, mut apply: F) -> CommandInterpreterResult<()> {
        let mut internal = || -> CommandInterpreterResult<()> {
            let (relative_metadata_path, abs_metadata_path) = self.get_note_metadata_path(&id);
            let note_metadata = &mut self.get_note_metadata_mut(&id)?;

            if apply(note_metadata) {
                note_metadata.save(&abs_metadata_path)?;

                let index = self.index()?;
                index.add_path(&relative_metadata_path)?;
                index.write()?;
            }

            Ok(())
        };

        internal().map_err(|err| CommandInterpreterError::FailedToUpdateMetadata(err.to_string()))
    }

    fn has_git_changes(&mut self) -> CommandInterpreterResult<bool> {
        let new_tree = self.index()?.write_tree()?;
        let new_tree = self.repository.find_tree(new_tree)?;

        let (_, head_tree) = CommandInterpreter::get_git_head(&self.repository)?;
        CommandInterpreter::has_git_diff(&self.repository, &head_tree, &new_tree)
    }

    fn get_git_head(repository: &git2::Repository) -> CommandInterpreterResult<(git2::Commit, git2::Tree)> {
        let head = repository.head()?;
        let head_commit = head.peel(git2::ObjectType::Commit)?;
        let head_commit = head_commit.as_commit().unwrap().clone();

        let head_tree = head.peel(git2::ObjectType::Tree)?;
        let head_tree = head_tree.as_tree().unwrap().clone();

        Ok((head_commit, head_tree))
    }

    fn has_git_diff(repository: &git2::Repository, head_tree: &git2::Tree, new_tree: &git2::Tree) -> CommandInterpreterResult<bool> {
        let diff = repository.diff_tree_to_tree(Some(&new_tree), Some(&head_tree), None)?;
        Ok(diff.stats()?.files_changed() > 0)
    }

    fn get_note_storage_path(&self, id: &NoteId) -> (PathBuf, PathBuf) {
        NoteMetadataStorage::get_note_storage_path(&self.config.repository, id)
    }

    fn get_note_metadata_path(&self, id: &NoteId) -> (PathBuf, PathBuf) {
        NoteMetadataStorage::get_note_metadata_path(&self.config.repository, id)
    }

    fn get_note_id(&mut self, path: &PathBuf) -> CommandInterpreterResult<NoteId> {
        self.note_metadata_storage()?
            .get_id(path)
            .ok_or_else(|| CommandInterpreterError::NoteNotFound(path.to_str().unwrap().to_owned()))
    }

    fn get_note_path(&mut self, id: &NoteId) -> CommandInterpreterResult<&Path> {
        self.note_metadata_storage()?
            .get_by_id(id)
            .map(|note| note.path.as_path())
            .ok_or_else(|| CommandInterpreterError::NoteNotFound(id.to_string()))
    }

    fn get_note_metadata_mut(&mut self, id: &NoteId) -> CommandInterpreterResult<&mut NoteMetadata> {
        self.note_metadata_storage_mut()?
            .get_by_id_mut(id)
            .ok_or_else(|| CommandInterpreterError::NoteNotFound(id.to_string()))
    }

    fn check_if_note_exists(&mut self, path: &Path) -> CommandInterpreterResult<()> {
        if self.note_metadata_storage()?.contains_path(path) {
            Err(CommandInterpreterError::NoteAlreadyExists(path.to_owned()))
        } else {
            Ok(())
        }
    }

    fn note_metadata_storage(&mut self) -> CommandInterpreterResult<&NoteMetadataStorage> {
        self.note_metadata_storage_mut().map(|x| &*x)
    }

    fn note_metadata_storage_mut(&mut self) -> CommandInterpreterResult<&mut NoteMetadataStorage> {
        if self.note_metadata_storage.is_some() {
            Ok(self.note_metadata_storage.as_mut().unwrap())
        } else {
            self.note_metadata_storage = Some(NoteMetadataStorage::from_dir(&self.config.repository)?);
            Ok(self.note_metadata_storage.as_mut().unwrap())
        }
    }

    fn index(&mut self) -> CommandInterpreterResult<&mut git2::Index> {
        CommandInterpreter::get_index(&self.repository, &mut self.index)
    }

    fn get_index<'a>(repository: &git2::Repository,
                     index: &'a mut Option<git2::Index>) -> CommandInterpreterResult<&'a mut git2::Index> {
        if index.is_some() {
            Ok(index.as_mut().unwrap())
        } else {
            *index = Some(repository.index()?);
            Ok(index.as_mut().unwrap())
        }
    }
}