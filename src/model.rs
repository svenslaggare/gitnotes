use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fmt::{Display};
use std::fs::File;
use std::io::{BufRead, BufReader, Lines};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use chrono::{DateTime, Local};

use fnv::FnvHashMap;

use rand::{Rng, thread_rng};

use serde::{Serialize, Deserialize, Deserializer, Serializer};
use serde::de::{Error, Visitor};

use crate::helpers::io_error;

const NOTE_ID_SIZE: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NoteId([char; NOTE_ID_SIZE]);
impl NoteId {
    pub fn new() -> NoteId {
        let mut rng = thread_rng();
        const CHARACTERS: [char; 10] = ['0', '1', '2', '3', '4', '5', '6', '7', '8', '9'];

        let mut id: [char; NOTE_ID_SIZE] = Default::default();
        for i in 0..NOTE_ID_SIZE {
            id[i] = CHARACTERS[rng.gen_range(0..CHARACTERS.len())];
        }

        NoteId(id)
    }

    pub fn from_vec(chars: Vec<char>) -> Option<NoteId> {
        if chars.len() == NOTE_ID_SIZE {
            let mut id: [char; NOTE_ID_SIZE] = Default::default();

            for i in 0..chars.len() {
                if !chars[i].is_numeric() {
                    return None;
                }

                id[i] = chars[i];
            }

            Some(NoteId(id))
        } else {
            None
        }
    }
}

impl Display for NoteId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for current in self.0 {
            write!(f, "{}", current)?
        }

        Ok(())
    }
}

const NOTE_ID_ERROR_MESSAGE: &str = "string of length 5 that only contains digits";

impl FromStr for NoteId {
    type Err = String;

    fn from_str(str: &str) -> Result<Self, Self::Err> {
        let chars = str.chars().collect();
        NoteId::from_vec(chars).ok_or_else(|| NOTE_ID_ERROR_MESSAGE.to_owned())
    }
}

struct NoteIdVisitor;
impl<'de> Visitor<'de> for NoteIdVisitor {
    type Value = NoteId;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str(NOTE_ID_ERROR_MESSAGE)
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E> where E: Error {
        self.visit_str(&value)
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E> where E: Error {
        let chars = value.chars().collect::<Vec<_>>();
        NoteId::from_vec(chars).ok_or_else(|| E::custom(NOTE_ID_ERROR_MESSAGE))
    }
}

impl<'de> Deserialize<'de> for NoteId {
    fn deserialize<D>(deserializer: D) -> Result<NoteId, D::Error> where D: Deserializer<'de> {
        deserializer.deserialize_string(NoteIdVisitor)
    }
}

impl Serialize for NoteId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error> where S: Serializer {
        serializer.serialize_str(&self.to_string())
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NoteMetadata {
    pub id: NoteId,
    pub created: DateTime<Local>,
    pub last_updated: DateTime<Local>,
    pub path: PathBuf,
    pub tags: Vec<String>
}

impl NoteMetadata {
    pub fn new(id: NoteId, path: PathBuf, tags: Vec<String>) -> NoteMetadata {
        let now = Local::now();
        NoteMetadata {
            id,
            created: now,
            last_updated: now,
            path,
            tags
        }
    }

    pub fn info_text(&self) -> String {
        format!("{} (id: {})", self.path.to_str().unwrap(), self.id)
    }

    pub fn load(path: &Path) -> std::io::Result<NoteMetadata> {
        let content = std::fs::read_to_string(path)?;
        toml::from_str(&content).map_err(|err| io_error(err))
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let toml = toml::to_string(self).map_err(|err| io_error(err))?;
        std::fs::write(path, toml)
    }

    pub fn load_all<F: FnMut(NoteMetadata)>(dir: &Path, mut apply: F) -> std::io::Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() && path.extension().unwrap().to_str() == Some("metadata") {
                apply(NoteMetadata::load(&path)?);
            }
        }

        Ok(())
    }

    pub fn load_all_to_vec(dir: &Path) -> std::io::Result<Vec<NoteMetadata>> {
        let mut values = Vec::new();

        NoteMetadata::load_all(dir, |note_metadata| {
            values.push(note_metadata);
        })?;

        Ok(values)
    }
}

pub struct NoteMetadataStorage {
    root_dir: PathBuf,
    id_to_notes: FnvHashMap<NoteId, NoteMetadata>,
    path_to_id: FnvHashMap<PathBuf, NoteId>
}

impl NoteMetadataStorage {
    pub fn from_dir(root_dir: &Path) -> std::io::Result<NoteMetadataStorage> {
        let mut path_to_id = FnvHashMap::default();
        let mut id_to_notes = FnvHashMap::default();

        NoteMetadata::load_all(root_dir, |note_metadata| {
            path_to_id.insert(note_metadata.path.clone(), note_metadata.id);
            id_to_notes.insert(note_metadata.id, note_metadata);
        })?;

        Ok(
            NoteMetadataStorage {
                root_dir: root_dir.to_path_buf(),
                path_to_id,
                id_to_notes
            }
        )
    }

    pub fn get_id(&self, path: &Path) -> Option<NoteId> {
        if let Ok(id) = NoteId::from_str(path.to_str().unwrap()) {
            if let Some(note) = self.id_to_notes.get(&id) {
                return Some(note.id);
            }
        }

        self.path_to_id.get(path).cloned()
    }

    pub fn get_id_result(&self, path: &Path) -> std::io::Result<NoteId> {
         self.get_id(path).ok_or_else(|| io_error(format!("Note '{}' not found", path.to_str().unwrap())))
    }

    pub fn get(&self, path: &Path) -> Option<&NoteMetadata> {
        self.id_to_notes.get(&self.get_id(path)?)
    }

    pub fn get_by_id(&self, id: &NoteId) -> Option<&NoteMetadata> {
        self.id_to_notes.get(id)
    }

    pub fn get_by_id_mut(&mut self, id: &NoteId) -> Option<&mut NoteMetadata> {
        self.id_to_notes.get_mut(id)
    }

    pub fn contains_path(&self, path: &Path) -> bool {
        self.path_to_id.contains_key(path)
    }

    pub fn notes(&self) -> impl Iterator<Item=&NoteMetadata> {
        self.id_to_notes.values()
    }

    pub fn get_content(&self, path: &Path) -> std::io::Result<String> {
        let id = self.get_id_result(path)?;
        let (_, abs_note_path) = NoteMetadataStorage::get_note_storage_path(&self.root_dir, &id);
        std::fs::read_to_string(abs_note_path)
    }

    pub fn get_content_lines(&self, path: &Path) -> std::io::Result<Lines<BufReader<File>>> {
        let id = self.get_id_result(path)?;
        let (_, abs_note_path) = NoteMetadataStorage::get_note_storage_path(&self.root_dir, &id);
        Ok(BufReader::new(File::open(abs_note_path)?).lines())
    }

    pub fn get_note_storage_path(root_dir: &Path, id: &NoteId) -> (PathBuf, PathBuf) {
        let relative_path = Path::new(&(id.to_string() + ".md")).to_path_buf();
        let abs_path = root_dir.join(&relative_path);
        (relative_path, abs_path)
    }

    pub fn get_note_metadata_path(root_dir: &Path, id: &NoteId) -> (PathBuf, PathBuf) {
        let relative_path = Path::new(&(id.to_string() + ".metadata")).to_path_buf();
        let abs_path = root_dir.join(&relative_path);
        (relative_path, abs_path)
    }
}

pub enum NoteFileTree<'a> {
    Note(&'a NoteMetadata),
    Tree {
        last_updated: Option<DateTime<Local>>,
        children: BTreeMap<OsString, NoteFileTree<'a>>
    }
}

impl<'a> NoteFileTree<'a> {
    pub fn new() -> NoteFileTree<'a> {
        NoteFileTree::Tree {
            last_updated: None,
            children: BTreeMap::new()
        }
    }

    pub fn with_updated(updated: DateTime<Local>) -> NoteFileTree<'a> {
        NoteFileTree::Tree {
            last_updated: Some(updated),
            children: BTreeMap::new()
        }
    }

    pub fn from_iter(iter: impl Iterator<Item=&'a NoteMetadata>) -> Option<NoteFileTree<'a>> {
        let mut root = NoteFileTree::new();

        for note_metadata in iter {
            let mut current = &mut root;

            let parts = note_metadata.path.iter().collect::<Vec<_>>();
            for (part_index, part) in parts.iter().enumerate() {
                let is_last = part_index == parts.len() - 1;
                match current {
                    NoteFileTree::Tree { last_updated, children } => {
                        let entry = children.entry(part.to_os_string()).or_insert_with(|| {
                            if is_last {
                                NoteFileTree::Note(note_metadata)
                            } else {
                                NoteFileTree::with_updated(note_metadata.last_updated)
                            }
                        });

                        if let Some(last_updated) = last_updated.as_mut() {
                            *last_updated = (*last_updated).max(note_metadata.last_updated);
                        } else {
                            *last_updated = Some(note_metadata.last_updated);
                        }

                        current = entry;
                    }
                    NoteFileTree::Note(_) => {
                        return None;
                    }
                }
            }
        }

        Some(root)
    }

    pub fn walk<F: FnMut(usize, &OsString, &'a NoteFileTree) -> bool>(&'a self, mut apply: F) {
        fn do_walk<'a, F: FnMut(usize, &OsString, &'a NoteFileTree) -> bool>(apply: &mut F, level: usize, tree: &'a NoteFileTree) {
            if let Some(children) = tree.children() {
                for (name, child) in children {
                    match child {
                        NoteFileTree::Note(_) => {
                            if !apply(level, name, child) {
                                return;
                            }
                        }
                        NoteFileTree::Tree { .. } => {
                            if !apply(level, name, child) {
                                return;
                            }

                            do_walk(apply, level + 1, child);
                        }
                    }
                }
            }
        }

        do_walk(&mut apply, 0, self);
    }

    pub fn find(&self, path: &Path) -> Option<&NoteFileTree> {
        let mut found = false;

        let mut current = self;
        for part in path.iter() {
            if let Some(children) = current.children() {
                if let Some(child) = children.get(part) {
                    current = child;
                    found = true;
                } else {
                    return None;
                }
            } else {
                return None;
            }
        }

        if found {
            Some(current)
        } else {
            None
        }
    }

    pub fn print(&self) {
        self.walk(
            |level, name, tree| {
                let padding = "  ".repeat(level);
                match tree {
                    NoteFileTree::Note(note_metadata) => {
                        println!("{}* {} (id: {})", padding, name.to_str().unwrap(), note_metadata.id);
                    }
                    NoteFileTree::Tree { .. } => {
                        println!("{}* {}", padding, name.to_str().unwrap());
                    }
                }

                true
            }
        );
    }

    fn children(&self) -> Option<&BTreeMap<OsString, NoteFileTree>> {
        if let NoteFileTree::Tree { children, .. } = self {
            Some(children)
        } else {
            None
        }
    }
}

macro_rules! assert_tree_eq {
    ($left:expr, $right:expr) => {
        {
           let mut results = Vec::new();
           $right.walk(|_, name, _| {
               results.push(name.to_str().unwrap().to_owned());
               true
           });

           assert_eq!(
               $left.iter().map(|x| x.to_owned()).collect::<Vec<_>>(),
               results
           );
        }
    };
}

#[test]
fn test_create_tree1() {
    let note_metadata = vec![
        NoteMetadata::new(NoteId::new(), Path::new("00.md").to_path_buf(), Vec::new()),
        NoteMetadata::new(NoteId::new(), Path::new("2023/01.md").to_path_buf(), Vec::new()),
        NoteMetadata::new(NoteId::new(), Path::new("2023/01/01/03.md").to_path_buf(), Vec::new()),
        NoteMetadata::new(NoteId::new(), Path::new("2023/01/01/04.md").to_path_buf(), Vec::new()),
        NoteMetadata::new(NoteId::new(), Path::new("2023/01/02/05.md").to_path_buf(), Vec::new()),
        NoteMetadata::new(NoteId::new(), Path::new("2023/02/01/06.md").to_path_buf(), Vec::new()),
        NoteMetadata::new(NoteId::new(), Path::new("2023/02.md").to_path_buf(), Vec::new()),
    ];

    let tree = NoteFileTree::from_iter(note_metadata.iter()).unwrap();
    tree.print();

    assert_tree_eq!(
        vec!["00.md", "2023", "01", "01", "03.md", "04.md", "02", "05.md", "01.md", "02", "01", "06.md", "02.md"],
        tree
    );
}

#[test]
fn test_find_tree1() {
    let note_metadata = vec![
        NoteMetadata::new(NoteId::new(), Path::new("00.md").to_path_buf(), Vec::new()),
        NoteMetadata::new(NoteId::new(), Path::new("2023/01.md").to_path_buf(), Vec::new()),
        NoteMetadata::new(NoteId::new(), Path::new("2023/01/01/03.md").to_path_buf(), Vec::new()),
        NoteMetadata::new(NoteId::new(), Path::new("2023/01/01/04.md").to_path_buf(), Vec::new()),
        NoteMetadata::new(NoteId::new(), Path::new("2023/01/02/05.md").to_path_buf(), Vec::new()),
        NoteMetadata::new(NoteId::new(), Path::new("2023/01/06.md").to_path_buf(), Vec::new()),
        NoteMetadata::new(NoteId::new(), Path::new("2023/02/01/07.md").to_path_buf(), Vec::new()),
        NoteMetadata::new(NoteId::new(), Path::new("2023/02.md").to_path_buf(), Vec::new()),
    ];

    let tree = NoteFileTree::from_iter(note_metadata.iter()).unwrap();

    let found = tree.find(Path::new("2023/01")).unwrap();
    found.print();

    assert_tree_eq!(
        vec!["00.md", "2023", "01", "01", "03.md", "04.md", "02", "05.md", "06.md", "01.md", "02", "01", "07.md", "02.md"],
        tree
    );
}