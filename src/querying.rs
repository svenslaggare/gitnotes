use std::collections::BTreeMap;
use std::io::{IsTerminal, stdout};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use chrono::{Datelike, DateTime, Local, Timelike};
use regex::{Regex};
use thiserror::Error;

use comrak::nodes::NodeValue;

use crossterm::ExecutableCommand;
use crossterm::style::{Color, Print, ResetColor, SetAttribute, SetForegroundColor};
use crossterm::style::Attribute::Bold;

use crate::helpers::{TablePrinter, ToChronoDateTime};
use crate::markdown;
use crate::model::{NOTE_CONTENT_EXT, NOTE_METADATA_EXT, NoteFileTree, NoteFileTreeCreateConfig, NoteMetadata, NoteMetadataStorage, NOTES_DIR};

pub const DATETIME_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

pub type QueryingResult<T> = Result<T, QueryingError>;

#[derive(Error, Debug)]
pub enum QueryingError {
    #[error("Failed to create note file tree")]
    FailedToCreateNoteFileTree,
    #[error("Note not found at git reference '{0}'")]
    NoteNotFoundAtGitReference(String),
    #[error("Note '{0}' not found")]
    NoteNotFound(String),
    #[error("Current tree is not a directory")]
    TreeNotDirectory,

    #[error("{0}")]
    Git(git2::Error),

    #[error("{0}")]
    IO(std::io::Error)
}

impl From<git2::Error> for QueryingError {
    fn from(err: git2::Error) -> Self {
        QueryingError::Git(err)
    }
}

impl From<std::io::Error> for QueryingError {
    fn from(err: std::io::Error) -> Self {
        QueryingError::IO(err)
    }
}

pub trait Matcher {
    fn is_match(&self, text: &str) -> bool;
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

        results.sort_by_key(|note_metadata| &note_metadata.path);
        Ok(results)
    }
}

pub fn print_note_metadata_results(results: &Vec<&NoteMetadata>) {
    let mut table_printer = TablePrinter::new(vec![
        "path".to_owned(),
        "id".to_owned(),
        "tags".to_owned(),
        "created".to_owned(),
        "last updated".to_owned(),
    ]);

    for note_metadata in results {
        table_printer.add_row(vec![
            note_metadata.path.to_str().unwrap().to_owned(),
            note_metadata.id.to_string(),
            note_metadata.tags.join(" "),
            note_metadata.created.format(DATETIME_FORMAT).to_string(),
            note_metadata.last_updated.format(DATETIME_FORMAT).to_string()
        ]);
    }

    table_printer.print();
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

    pub fn search(&self, query: &Regex) -> QueryingResult<Vec<&'a NoteMetadata>> {
        let is_terminal = stdout().is_terminal();

        let mut matches = Vec::new();
        for note_metadata in self.note_metadata_storage.notes() {
            for line in self.note_metadata_storage.get_content_lines(&note_metadata.path)? {
                let line = line?;

                self.find_matches(
                    query,
                    &line,
                    is_terminal,
                    |is_terminal| {
                        let info_text = note_metadata.info_text();
                        if is_terminal {
                            stdout()
                                .execute(SetForegroundColor(Color::DarkMagenta))?
                                .execute(Print(format!("{}: ", info_text)))?
                                .execute(ResetColor)?;
                        } else {
                            print!("{}: ", info_text);
                        }

                        matches.push(note_metadata);
                        Ok(())
                    }
                )?;
            }
        }

        Ok(matches)
    }

    pub fn search_historic(&self,
                           repository: &git2::Repository,
                           query: &Regex,
                           git_start: &str, git_end: Option<&str>) -> QueryingResult<Vec<(git2::Oid, NoteMetadata)>> {
        let is_terminal = stdout().is_terminal();

        let mut rev_walk = repository.revwalk()?;
        rev_walk.push(repository.revparse_single(git_start)?.id())?;

        if let Some(git_end) = git_end {
            rev_walk.hide(repository.revparse_single(git_end)?.id())?;
        }

        let mut matches = Vec::new();
        for commit_id in rev_walk {
            let commit_id = commit_id?;
            let commit = repository.find_commit(commit_id)?;
            let tree = commit.tree()?;

            let mut notes = BTreeMap::new();
            for file_entry in tree.iter() {
                let file_path = Path::new(file_entry.name().unwrap());
                let note_id = file_path.file_stem().unwrap().to_os_string();

                let note_entry = notes.entry(note_id).or_insert_with(|| (None, None));
                match file_path.extension().map(|x| x.to_str().unwrap()) {
                    Some(entry) if entry == NOTE_METADATA_EXT => {
                        note_entry.0 = Some(file_entry);
                    }
                    Some(entry) if entry == NOTE_CONTENT_EXT => {
                        note_entry.1 = Some(file_entry);
                    }
                    _ => {}
                }
            }

            for note_entry in notes.values() {
                if let (Some(metadata_entry), Some(content_entry)) = note_entry {
                    let metadata_entry = metadata_entry.to_object(&repository)?;
                    let metadata_content = metadata_entry
                        .as_blob()
                        .map(|blob| std::str::from_utf8(blob.content()).ok())
                        .flatten();

                    let content_entry = content_entry.to_object(&repository)?;
                    let content = content_entry
                        .as_blob()
                        .map(|blob| std::str::from_utf8(blob.content()).ok())
                        .flatten();

                    if let (Some(metadata_content), Some(content)) = (metadata_content, content) {
                        let note_metadata = NoteMetadata::parse(metadata_content)?;

                        for line in content.lines() {
                            self.find_matches(
                                query,
                                line,
                                is_terminal,
                                |is_terminal| {
                                    matches.push((commit_id, note_metadata.clone()));

                                    let info_text = note_metadata.info_text();
                                    let short_commit_id = commit.as_object().short_id()?.as_str().unwrap().to_owned();

                                    if is_terminal {
                                        stdout()
                                            .execute(SetForegroundColor(Color::Yellow))?
                                            .execute(Print(format!("{}", short_commit_id)))?
                                            .execute(ResetColor)?

                                            .execute(Print(format!(" - ")))?

                                            .execute(SetForegroundColor(Color::DarkMagenta))?
                                            .execute(Print(format!("{}: ", info_text)))?
                                            .execute(ResetColor)?;
                                    } else {
                                        print!("{} - {}: ", short_commit_id, info_text);
                                    }

                                    Ok(())
                                }
                            )?;
                        }
                    }
                }
            }
        }

        Ok(matches)
    }

    fn find_matches<FnFirst: FnMut(bool) -> QueryingResult<()>>(
        &self,
        query: &Regex, line: &str,
        is_terminal: bool,
        mut before_first: FnFirst,
    ) -> QueryingResult<()> {
        let mut remaining_line_start = 0;
        let mut found_match = false;
        for current_match in query.find_iter(&line) {
            if !found_match {
                before_first(is_terminal)?;
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

    pub fn list(&'a self, query: &Path) -> QueryingResult<Vec<ListDirectoryEntry<'a>>> {
        let mut results = Vec::new();

        let found_tree = if query == Path::new("") {
            Some(&self.root)
        } else {
            self.root.find(query)
        };

        if let Some(found_tree) = found_tree {
            if found_tree.is_leaf() {
                return Err(QueryingError::TreeNotDirectory);
            }

            found_tree.walk(|level, _, name, tree, _| {
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
        } else {
            return Err(QueryingError::NoteNotFound(query.to_str().unwrap_or("").to_owned()));
        }

        Ok(results)
    }
}

pub fn print_list_directory_results(results: &Vec<ListDirectoryEntry>) -> QueryingResult<()> {
    let is_terminal = stdout().is_terminal();

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

pub struct ListTree<'a> {
    root: NoteFileTree<'a>
}

impl<'a> ListTree<'a> {
    pub fn new(note_metadata_storage: &'a NoteMetadataStorage, config: NoteFileTreeCreateConfig) -> QueryingResult<ListTree<'a>> {
        Ok(
            ListTree {
                root: NoteFileTree::from_iter_with_config(
                    note_metadata_storage.notes(),
                    config
                ).ok_or_else(|| QueryingError::FailedToCreateNoteFileTree)?
            }
        )
    }

    pub fn list(&self, prefix: &Path) {
        if prefix == Path::new("") {
            ListTree::print_tree(&self.root, ".")
        } else {
            if let Some(tree) = self.root.find(prefix) {
                ListTree::print_tree(&tree, prefix.to_str().unwrap());
            }
        }
    }

    pub fn print_tree(tree: &NoteFileTree, dir: &str) {
        let is_terminal = stdout().is_terminal();

        if !dir.is_empty() {
            if is_terminal {
                stdout()
                    .execute(SetForegroundColor(Color::Blue)).unwrap()
                    .execute(Print(dir)).unwrap()
                    .execute(ResetColor).unwrap()
                    .execute(Print("\n")).unwrap();
            } else {
                println!("{}", dir);
            }
        }

        tree.walk(
            |_, _, name, tree, stack| {
                for current in stack.is_last_stack.iter() {
                    if !current {
                        print!("│   ");
                    } else {
                        print!("    ");
                    }
                }

                print!("{}── ", if stack.is_last {"└"} else {"├"});
                let (content, color) = match tree {
                    NoteFileTree::Note(note_metadata) => {
                        (format!("{} (id: {})", name.to_str().unwrap(), note_metadata.id), Color::Green)
                    }
                    NoteFileTree::Tree { .. } => {
                        (format!("{}", name.to_str().unwrap().to_owned()), Color::Blue)
                    }
                };

                if is_terminal {
                    stdout()
                        .execute(SetForegroundColor(color)).unwrap()
                        .execute(Print(content)).unwrap()
                        .execute(ResetColor).unwrap()
                        .execute(Print("\n")).unwrap();
                } else {
                    println!("{}", content);
                }

                true
            }
        );
    }
}

pub fn list_resources(base_dir: &Path,
                      query: Option<PathBuf>,
                      print_absolute: bool) -> QueryingResult<()> {
    println!("Resources:");
    let mut initial_search_dir = base_dir.to_owned();
    if let Some(query) = query {
        initial_search_dir = base_dir.join(query);
    }

    let mut stack = vec![initial_search_dir];
    'outer:
    while let Some(top) = stack.pop() {
        for entry in std::fs::read_dir(top)? {
            if let Ok(entry) = entry {
                let path = entry.path();
                if path.is_file() {
                    let relative_path = path.strip_prefix(&base_dir).unwrap();
                    let path_to_use = if print_absolute { &path } else { relative_path };

                    println!("{}", path_to_use.to_str().unwrap());
                } else {
                    stack.push(entry.path());
                }
            } else {
                break 'outer;
            }
        }
    }

    Ok(())
}

pub struct GitLog<'a> {
    repository: &'a git2::Repository,
    count: isize
}

impl<'a> GitLog<'a> {
    pub fn new(repository: &'a git2::Repository, count: isize) -> QueryingResult<GitLog> {
        Ok(
            GitLog {
                repository,
                count
            }
        )
    }

    pub fn print(&self) -> QueryingResult<()> {
        let mut rev_walk = self.repository.revwalk()?;
        rev_walk.push_head()?;

        for commit_id in rev_walk.into_iter().take(if self.count >= 0 { self.count as usize } else { usize::MAX }) {
            let commit_id = commit_id?;
            let commit = self.repository.find_commit(commit_id)?;

            let short_commit_hash = commit.as_object().short_id()?.as_str().unwrap().to_owned();
            let commit_time = commit.time().to_date_time().unwrap();

            println!(
                "{} ({}): {}",
                short_commit_hash,
                commit_time.format(DATETIME_FORMAT),
                commit.message().unwrap_or("").trim().replace("\n", " ")
            );
        }

        Ok(())
    }
}

pub fn get_note_content(repository: &git2::Repository,
                        note_metadata_storage: &NoteMetadataStorage,
                        path: &Path, git_reference: Option<String>) -> QueryingResult<String> {
    if let Some(git_reference) = git_reference {
        let git_content_fetcher = GitContentFetcher::new(repository, note_metadata_storage);

        if let Some(commit_content) = git_content_fetcher.fetch(&path, &git_reference)? {
            Ok(commit_content)
        } else {
            Err(QueryingError::NoteNotFoundAtGitReference(git_reference))
        }
    } else {
        Ok(note_metadata_storage.get_content(&path)?)
    }
}

pub struct GitContentFetcher<'a> {
    repository: &'a git2::Repository,
    node_metadata_storage: &'a NoteMetadataStorage
}

impl<'a> GitContentFetcher<'a> {
    pub fn new(repository: &'a git2::Repository, node_metadata_storage: &'a NoteMetadataStorage) -> GitContentFetcher<'a> {
        GitContentFetcher {
            repository,
            node_metadata_storage
        }
    }

    pub fn fetch(&self, path: &Path, spec: &str) -> QueryingResult<Option<String>> {
        let note_id = self.node_metadata_storage.get_id_result(&path)?;

        let git_id = self.repository.revparse_single(spec)?.id();
        let tree = self.repository.find_commit(git_id)?.tree()?;

        if let Ok(entry) = tree.get_path(Path::new(&format!("{}/{}.{}", NOTES_DIR, note_id.to_string(), NOTE_CONTENT_EXT))) {
            let entry_object = entry.to_object(&self.repository)?;
            if let Some(entry_blob) = entry_object.as_blob() {
                return Ok(Some(String::from_utf8_lossy(entry_blob.content()).to_string()))
            }
        }

        Ok(None)
    }
}

pub fn extract_content(content: String, only_code: bool, only_output: bool) -> QueryingResult<String> {
    if only_code || only_output {
        let arena = markdown::storage();
        let root = markdown::parse(&arena, &content);

        let mut new_content = String::new();
        markdown::visit_code_blocks::<QueryingError, _>(
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

        Ok(new_content)
    } else {
        Ok(content)
    }
}

#[derive(Debug)]
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

impl FromStr for StringMatcher {
    type Err = String;

    fn from_str(str: &str) -> Result<Self, Self::Err> {
        Ok(StringMatcher::new(str))
    }
}

#[derive(Debug)]
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

impl FromStr for RegexMatcher {
    type Err = regex::Error;

    fn from_str(str: &str) -> Result<Self, Self::Err> {
        match Regex::new(str) {
            Ok(regex) => Ok(RegexMatcher(regex)),
            Err(err) => Err(err)
        }
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