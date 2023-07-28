use std::error;

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