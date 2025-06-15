use std::{
    fmt::{Display, Formatter, Result as FmtResult},
    sync::PoisonError,
};

use log::SetLoggerError;
use tokio::sync::mpsc::error::SendError;

#[derive(Debug)]
pub enum Error {
    SetLogger(String),
    LogKv(String),
    LockPoison(String),
    Io(String),
    Send(String),
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        match self {
            Self::SetLogger(e) => write!(f, "{}", e),
            Self::LogKv(e) => write!(f, "{}", e),
            Self::LockPoison(e) => write!(f, "{}", e),
            Self::Io(e) => write!(f, "{}", e),
            Self::Send(e) => write!(f, "{}", e),
        }
    }
}

impl std::error::Error for Error {}

impl<T> From<PoisonError<T>> for Error {
    fn from(value: PoisonError<T>) -> Self {
        Self::LockPoison(value.to_string())
    }
}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value.to_string())
    }
}

impl From<log::kv::Error> for Error {
    fn from(value: log::kv::Error) -> Self {
        Self::LogKv(value.to_string())
    }
}

impl From<SendError<()>> for Error {
    fn from(value: SendError<()>) -> Self {
        Self::Send(value.to_string())
    }
}

impl<T> From<crossbeam_channel::SendError<T>> for Error {
    fn from(value: crossbeam_channel::SendError<T>) -> Self {
        Self::Send(value.to_string())
    }
}

impl From<SetLoggerError> for Error {
    fn from(value: SetLoggerError) -> Self {
        Self::SetLogger(value.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
