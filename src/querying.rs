use std::io::stdout;
use std::path::Path;
use atty::Stream;

use chrono::{Datelike, DateTime, Local, Timelike};
use regex::Regex;
use thiserror::Error;

use crossterm::ExecutableCommand;
use crossterm::style::{Color, Print, ResetColor, SetAttribute, SetForegroundColor};
use crossterm::style::Attribute::Bold;

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
        RegexMatcher(Regex::new(str).expect("Invalid regex."))
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
    Created(Vec<i32>),
    LastUpdated(Vec<i32>)
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
                is_datetime_match(&note_metadata.created, parts)
            }
            FindQuery::LastUpdated(parts) => {
                is_datetime_match(&note_metadata.last_updated, parts)
            }
        }
    }
}

fn is_datetime_match(datetime: &DateTime<Local>, parts: &Vec<i32>) -> bool {
    fn is_part_match(value: i32, part: Option<&i32>) -> bool {
        if part.is_some() {
            &value == part.unwrap()
        } else {
            true
        }
    }

    is_part_match(datetime.year(), parts.get(0))
    && is_part_match(datetime.month() as i32, parts.get(1))
    && is_part_match(datetime.day() as i32, parts.get(2))
    && is_part_match(datetime.hour() as i32, parts.get(3))
    && is_part_match(datetime.minute() as i32, parts.get(4))
    && is_part_match(datetime.second() as i32, parts.get(5))
}

pub struct Finder<'a> {
    note_metadata_storage: &'a NoteMetadataStorage
}

impl<'a> Finder<'a> {
    pub fn new(note_metadata_storage: &'a NoteMetadataStorage) -> QueryingResult<Finder<'a>> {
        Ok(
            Finder {
                note_metadata_storage
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

pub struct Searcher<'a> {
    note_metadata_storage: &'a NoteMetadataStorage
}

impl<'a> Searcher<'a> {
    pub fn new(note_metadata_storage: &'a NoteMetadataStorage) -> QueryingResult<Searcher<'a>> {
        Ok(
            Searcher {
                note_metadata_storage
            }
        )
    }

    pub fn search(&self, query: &Regex) -> QueryingResult<()> {
        let is_terminal = atty::is(Stream::Stdout);

        for note_metadata in self.note_metadata_storage.notes() {
            for line in self.note_metadata_storage.get_content_lines(&note_metadata.path)? {
                let line = line?;

                let mut remaining_line_start = 0;
                let mut found_match = false;
                for current_match in query.find_iter(&line) {
                    if !found_match {
                        if is_terminal {
                            stdout()
                                .execute(SetForegroundColor(Color::DarkMagenta))?
                                .execute(Print(format!("{}: ", note_metadata.info_text())))?
                                .execute(ResetColor)?;
                        } else {
                            print!("{}: ", note_metadata.info_text());
                        }

                        found_match = true;
                    }

                    let before = &line[remaining_line_start..current_match.start()];
                    let during = &line[current_match.range()];
                    remaining_line_start = current_match.end();

                    if is_terminal {
                        stdout()
                            .execute(Print(before))?

                            .execute(SetAttribute(Bold))?
                            .execute(SetForegroundColor(Color::Red))?
                            .execute(Print(during))?
                            .execute(ResetColor)?;
                    } else {
                        print!("{}{}", before, during);
                    }
                }

                if found_match {
                    if is_terminal {
                        stdout()
                            .execute(Print(&line[remaining_line_start..]))?
                            .execute(Print("\n"))?;
                    } else {
                        println!("{}", &line[remaining_line_start..]);
                    }
                }
            }
        }

        Ok(())
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
    pub fn new(note_metadata_storage: &'a NoteMetadataStorage) -> QueryingResult<ListDirectory<'a>> {
        Ok(
            ListDirectory {
                root: NoteFileTree::from_iter(note_metadata_storage.notes()).ok_or_else(|| QueryingError::FailedToCreateNoteFileTree)?
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
    pub fn new(note_metadata_storage: &'a NoteMetadataStorage) -> QueryingResult<ListTree<'a>> {
        Ok(
            ListTree {
                root: NoteFileTree::from_iter(note_metadata_storage.notes()).ok_or_else(|| QueryingError::FailedToCreateNoteFileTree)?
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
        println!(
            "{} - id: {}, created: {}, last updated: {}",
            note_metadata.path.to_str().unwrap(),
            note_metadata.id,
            note_metadata.created,
            note_metadata.last_updated
        );
    }
}

pub fn print_list_directory_results(results: &Vec<ListDirectoryEntry>) -> QueryingResult<()> {
    let is_terminal = atty::is(Stream::Stdout);

    for entry in results {
        let last_updated = entry.last_updated.unwrap();

        let date_part = format!(
            "{}-{:0>2}-{:0>2} {:0>2}:{:0>2}\t{}\t",
            last_updated.year(),
            last_updated.month(),
            last_updated.day(),
            last_updated.hour(),
            last_updated.minute(),
            entry.note_metadata.map(|_| "note").unwrap_or("dir"),
        );

        let name_part = format!(
            "{}{}",
            entry.name,
            entry.note_metadata.map(|metadata| format!(" (id: {})", metadata.id)).unwrap_or_else(|| String::new())
        );

        if is_terminal {
            stdout()
                .execute(Print(date_part))?
                .execute(SetForegroundColor(if entry.note_metadata.is_some() { Color::Green } else { Color::Blue }))?
                .execute(Print(name_part))?
                .execute(ResetColor)?
                .execute(Print("\n"))?;
        } else {
            println!("{}{}", date_part, name_part);
        }
    }

    Ok(())
}