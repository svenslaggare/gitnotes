use std::error;
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