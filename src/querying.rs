use std::path::Path;
use chrono::{Datelike, DateTime, Local, Timelike};
use regex::Regex;

use thiserror::Error;

use crate::model::{NoteFileTree, NoteMetadata, NoteMetadataStorage};

pub type QueryingResult<T> = Result<T, QueryingError>;

#[derive(Error, Debug)]
pub enum QueryingError {
    #[error("Failed to create note file tree")]
    FailedToCreateNoteFileTree,

    #[error("I/O error: {0}")]
    IO(std::io::Error)
}

impl From<std::io::Error> for QueryingError {
    fn from(err: std::io::Error) -> Self {
        QueryingError::IO(err)
    }
}

pub trait Matcher {
    fn is_match(&self, text: &str) -> bool;
}

pub struct StringMatcher(String);
impl StringMatcher {
    pub fn new(str: &str) -> StringMatcher {
        StringMatcher(str.to_owned())
    }
}

impl Matcher for StringMatcher {
    fn is_match(&self, text: &str) -> bool {
        self.0 == text
    }
}

pub struct RegexMatcher(Regex);
impl RegexMatcher {
    pub fn new(str: &str) -> RegexMatcher {
        RegexMatcher(Regex::new(str).unwrap())
    }
}

impl Matcher for RegexMatcher {
    fn is_match(&self, text: &str) -> bool {
        self.0.is_match(text)
    }
}

pub enum FindQuery {
    Tags(Vec<StringMatcher>),
    Path(RegexMatcher),
    Id(RegexMatcher),
    Created(Vec<i32>)
}

impl FindQuery {
    pub fn is_match(&self, note_metadata: &NoteMetadata) -> bool {
        match self {
            FindQuery::Tags(tags) => {
                for tag in tags {
                    if !note_metadata.tags.iter().any(|current_tag| tag.is_match(current_tag)) {
                        return false;
                    }
                }

                true
            }
            FindQuery::Path(path) => {
                path.is_match(note_metadata.path.to_str().unwrap())
            }
            FindQuery::Id(id) => {
                id.is_match(&note_metadata.id.to_string())
            }
            FindQuery::Created(parts) => {
                fn is_part_match(value: i32, part: Option<&i32>) -> bool {
                    if part.is_some() {
                        &value == part.unwrap()
                    } else {
                        true
                    }
                }

                is_part_match(note_metadata.created.year(), parts.get(0))
                && is_part_match(note_metadata.created.month() as i32, parts.get(1))
                && is_part_match(note_metadata.created.day() as i32, parts.get(2))
                && is_part_match(note_metadata.created.hour() as i32, parts.get(3))
                && is_part_match(note_metadata.created.minute() as i32, parts.get(4))
                && is_part_match(note_metadata.created.second() as i32, parts.get(5))
            }
        }
    }
}

pub struct Finder {
    note_metadata_storage: NoteMetadataStorage
}

impl Finder {
    pub fn new(repository_path: &Path) -> QueryingResult<Finder> {
        Ok(
            Finder {
                note_metadata_storage: NoteMetadataStorage::from_dir(repository_path)?
            }
        )
    }

    pub fn find(&self, query: &FindQuery) -> QueryingResult<Vec<&NoteMetadata>> {
        let mut results = Vec::new();

        for note_metadata in self.note_metadata_storage.notes() {
            if query.is_match(note_metadata) {
                results.push(note_metadata);
            }
        }

        Ok(results)
    }
}

pub struct ListDirectoryEntry<'a> {
    pub name: String,
    pub last_updated: Option<DateTime<Local>>,
    pub note_metadata: Option<&'a NoteMetadata>
}

pub struct ListDirectory<'a> {
    root: NoteFileTree<'a>
}

impl<'a> ListDirectory<'a> {
    pub fn new(notes_metadata: &'a Vec<NoteMetadata>) -> QueryingResult<ListDirectory<'a>> {
        Ok(
            ListDirectory {
                root: NoteFileTree::from_iter(notes_metadata.iter()).ok_or_else(|| QueryingError::FailedToCreateNoteFileTree)?
            }
        )
    }

    pub fn list(&'a self, query: Option<&str>) -> Vec<ListDirectoryEntry<'a>> {
        let mut results = Vec::new();

        let found_tree = if let Some(query) = query {
            self.root.find(&Path::new(query))
        } else {
            Some(&self.root)
        };

        if let Some(found_tree) = found_tree {
            found_tree.walk(|level, name, tree| {
                if level != 0 {
                    return false;
                }

                results.push(
                    match tree {
                        NoteFileTree::Note(metadata) => {
                            ListDirectoryEntry::<'a> {
                                name: name.to_str().unwrap().to_owned(),
                                last_updated: Some(metadata.last_updated),
                                note_metadata: Some(*metadata)
                            }
                        }
                        NoteFileTree::Tree { last_updated, .. } => {
                            ListDirectoryEntry::<'a> {
                                name: name.to_str().unwrap().to_owned(),
                                last_updated: *last_updated,
                                note_metadata: None
                            }
                        }
                    }
                );

                true
            });
        }

        results
    }
}

pub struct ListTree<'a> {
    root: NoteFileTree<'a>
}

impl<'a> ListTree<'a> {
    pub fn new(notes_metadata: &'a Vec<NoteMetadata>) -> QueryingResult<ListTree<'a>> {
        Ok(
            ListTree {
                root: NoteFileTree::from_iter(notes_metadata.iter()).ok_or_else(|| QueryingError::FailedToCreateNoteFileTree)?
            }
        )
    }

    pub fn list(&self, prefix: Option<&Path>) {
        match prefix {
            None => {
                self.root.print();
            }
            Some(prefix) => {
                if let Some(tree) = self.root.find(prefix) {
                    tree.print();
                }
            }
        }
    }
}

pub fn print_note_metadata_results(results: &Vec<&NoteMetadata>) {
    for note_metadata in results {
        println!("{} - id: {}, created: {}, last updated: {}", note_metadata.path.to_str().unwrap(), note_metadata.id, note_metadata.created, note_metadata.last_updated);
    }
}

pub fn print_list_directory_results(results: &Vec<ListDirectoryEntry>) {
    for entry in results {
        let last_updated = entry.last_updated.unwrap();
        println!(
            "{}-{:0>2}-{:0>2} {:0>2}:{:0>2}\t{}\t{}{}",
            last_updated.year(),
            last_updated.month(),
            last_updated.day(),
            last_updated.hour(),
            last_updated.minute(),
            entry.note_metadata.map(|_| "note").unwrap_or("dir"),
            entry.name,
            entry.note_metadata.map(|metadata| format!(" (id: {})", metadata.id)).unwrap_or_else(|| String::new())
        );
    }
}