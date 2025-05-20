use thiserror::Error;
use std::io;

#[derive(Error, Debug)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("Invalid channel ID: {0}")]
    InvalidChannel(u8),
}