//! Parsers for files from outside: platform reports and bank statements. Pure functions:
//! bytes in, structured data out. Nothing here touches the database.

pub mod apple;
pub mod bank;
pub mod google;
mod text;

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ImportError {
    #[error("bestand is leeg")]
    Empty,
    #[error("kolom ontbreekt: {0}")]
    MissingColumn(String),
    #[error("regel {line}: {message}")]
    Row { line: usize, message: String },
    #[error("onbekend bestandsformaat: {0}")]
    UnknownFormat(String),
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, ImportError>;
