use std::fmt;

#[derive(Debug)]
pub enum Error {
    NotCfb,
    NotJtd(String),
    Corrupt(String),
    Unsupported(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotCfb => write!(f, "not a compound file (CFB/OLE2)"),
            Error::NotJtd(m) => write!(f, "not an Ichitaro document: {m}"),
            Error::Corrupt(m) => write!(f, "corrupt file: {m}"),
            Error::Unsupported(m) => write!(f, "unsupported: {m}"),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
