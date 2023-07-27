use std::error;

pub fn io_error<E: Into<Box<dyn error::Error + Send + Sync>>>(err: E) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Other, err)
}