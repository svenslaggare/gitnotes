use std::collections::{HashSet};
use std::error;
use std::hash::{Hash, Hasher};
use std::io::{Read, Stdin};
use std::path::PathBuf;

use chrono::{DateTime, FixedOffset, NaiveDateTime, TimeZone};

use home::home_dir;

pub fn base_dir() -> PathBuf {
    let home = home_dir().expect("Unable to determine home folder.");
    home.join(".gitnotes")
}

pub fn io_error<E: Into<Box<dyn error::Error + Send + Sync>>>(err: E) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Other, err)
}

pub fn get_or_insert_with<T, E, F: Fn() -> Result<T, E>>(option: &mut Option<T>, create: F) -> Result<&mut T, E> {
    if option.is_some() {
        Ok(option.as_mut().unwrap())
    } else {
        *option = Some(create()?);
        Ok(option.as_mut().unwrap())
    }
}

pub trait ToChronoDateTime {
    fn to_date_time(&self) -> Option<DateTime<FixedOffset>>;
}

impl ToChronoDateTime for git2::Time {
    fn to_date_time(&self) -> Option<DateTime<FixedOffset>> {
        let time = NaiveDateTime::from_timestamp_opt(self.seconds(), 0)?;
        Some(FixedOffset::east_opt(self.offset_minutes() * 60).unwrap().from_utc_datetime(&time))
    }
}

pub trait StdinExt {
    fn read_into_string(&mut self) -> std::io::Result<String>;
}

impl StdinExt for Stdin {
    fn read_into_string(&mut self) -> std::io::Result<String> {
        let mut content = String::new();
        self.read_to_string(&mut content)?;
        Ok(content)
    }
}

pub struct OrderedSet<T> where T: Eq + Hash {
    set: HashSet<PointerValueEquality<T>>,
    values: Vec<Box<T>>
}

impl<T> OrderedSet<T> where T: Eq + Hash {
    pub fn new() -> OrderedSet<T> {
        OrderedSet {
            set: HashSet::default(),
            values: Vec::new()
        }
    }

    pub fn iter(&self) -> impl Iterator<Item=&T> {
        self.values.iter().map(|x| x.as_ref())
    }

    pub fn into_iter(self) -> impl Iterator<Item=T> {
        self.values.into_iter().map(|value| *value)
    }

    pub fn len(&self) -> usize {
        self.set.len()
    }

    pub fn contains(&self, value: &T) -> bool {
        self.set.contains(&PointerValueEquality(value))
    }

    pub fn insert(&mut self, value: T) -> bool {
        if !self.contains(&value) {
            self.values.push(Box::new(value));
            self.set.insert(PointerValueEquality(self.values.last().unwrap().as_ref()));
            true
        } else {
            false
        }
    }

    pub fn clear(&mut self) {
        self.set.clear();
        self.values.clear();
    }
}

impl<T: Eq + Hash> Default for OrderedSet<T> {
    fn default() -> Self {
        OrderedSet::new()
    }
}

struct PointerValueEquality<T: Eq + Hash>(*const T);

impl<T: Eq + Hash> PartialEq for PointerValueEquality<T> {
    fn eq(&self, other: &Self) -> bool {
        unsafe {
            let self_ref = self.0.as_ref().unwrap();
            let other_ref = other.0.as_ref().unwrap();
            self_ref.eq(other_ref)
        }
    }
}
impl<T: Eq + Hash> Eq for PointerValueEquality<T> {}

impl<T: Eq + Hash> Hash for PointerValueEquality<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        unsafe {
            (*self.0).hash(state)
        }
    }
}

#[test]
fn test_ordered_set1() {
    let mut set = OrderedSet::new();
    assert!(set.insert(1));
    assert!(set.insert(2));
    assert!(set.insert(3));
    assert!(!set.insert(2));
    assert!(set.insert(4));
    assert!(!set.insert(1));
    assert!(set.insert(5));

    assert_eq!(5, set.len());
    assert_eq!(vec![1, 2, 3, 4, 5], set.iter().cloned().collect::<Vec<_>>());
    assert_eq!(true, set.contains(&1));
    assert_eq!(true, set.contains(&2));
    assert_eq!(false, set.contains(&6));

    assert_eq!(vec![1, 2, 3, 4, 5], set.into_iter().collect::<Vec<_>>());
}

#[test]
fn test_ordered_set2() {
    let mut set = OrderedSet::new();
    for iteration in 0..5 {
        for value in 0..200 {
            if iteration == 0 {
                assert!(set.insert(value));
            } else {
                assert!(!set.insert(value));
            }
        }
    }

    assert_eq!(200, set.len());
    assert_eq!((0..200).collect::<Vec<_>>(), set.iter().cloned().collect::<Vec<_>>());
    for value in 0..200 {
        assert!(set.contains(&value));
    }

    for value in 200..250 {
        assert!(!set.contains(&value));
    }
}