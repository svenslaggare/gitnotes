use std::collections::HashSet;
use std::error;
use std::hash::Hash;
use std::path::PathBuf;
use std::sync::Arc;

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

pub struct OrderedSet<T> where T: Eq + Hash {
    values: HashSet<Arc<T>>,
    ordering: Vec<Arc<T>>
}

impl<T> OrderedSet<T> where T: Eq + Hash {
    pub fn new() -> OrderedSet<T> {
        OrderedSet {
            values: HashSet::default(),
            ordering: vec![],
        }
    }

    pub fn iter(&self) -> impl Iterator<Item=&T> {
        self.ordering.iter().map(|x| x.as_ref())
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn contains(&self, value: &T) -> bool {
        self.values.contains(value)
    }

    pub fn insert(&mut self, value: T) -> bool {
        let value = Arc::new(value);
        if self.values.insert(value.clone()) {
            self.ordering.push(value);
            true
        } else {
            false
        }
    }

    pub fn clear(&mut self) {
        self.values.clear();
        self.ordering.clear();
    }
}

#[test]
fn test_ordered_set1() {
    let mut set = OrderedSet::new();
    set.insert(1);
    set.insert(2);
    set.insert(3);
    set.insert(2);
    set.insert(4);
    set.insert(1);
    set.insert(5);

    assert_eq!(5, set.len());
    assert_eq!(vec![1, 2, 3, 4, 5], set.iter().cloned().collect::<Vec<_>>());
    assert_eq!(true, set.contains(&1));
    assert_eq!(true, set.contains(&2));
    assert_eq!(false, set.contains(&6));
}