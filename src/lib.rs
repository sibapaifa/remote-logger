use std::sync::Arc;

use log::{LevelFilter, Log};

use crate::{append::Append, error::Result};

mod append;
mod error;
mod kvs;
mod log_message;

pub use crate::append::ConsoleAppender;
pub use crate::append::FileAppender;
pub use crate::append::RemoteAppender;
pub use crate::append::SendLog;
pub use crate::log_message::FormatMessage;
pub use crate::log_message::LogMessage;

type AppendersList = Arc<Vec<Box<dyn Append>>>;

#[must_use = "method `close_all` must be called before exit"]
pub struct LoggerHandle {
    appenders: AppendersList,
}

impl LoggerHandle {
    pub fn new(appenders: AppendersList) -> Self {
        Self { appenders }
    }

    pub async fn close_all(self) -> Result<()> {
        for appender in self.appenders.iter() {
            appender.close().await?;
        }

        Ok(())
    }
}

pub struct RemoteLogger {
    max_level: LevelFilter,
    appenders: AppendersList,
}

impl Default for RemoteLogger {
    fn default() -> Self {
        Self::new()
    }
}

impl RemoteLogger {
    pub fn new() -> Self {
        let appenders = vec![];
        let appenders = Arc::new(appenders);
        let max_level = LevelFilter::Trace;
        Self {
            max_level,
            appenders,
        }
    }

    pub fn with_appender<A: Append>(mut self, appender: A) -> Self {
        let appender = Box::new(appender);
        if let Some(appenders) = Arc::get_mut(&mut self.appenders) {
            appenders.push(appender);
        }
        self
    }

    pub fn with_max_level(mut self, level: LevelFilter) -> Self {
        self.max_level = level;
        self
    }

    pub fn init(self) -> Result<LoggerHandle> {
        let appenders = self.appenders.clone();
        let handle = LoggerHandle { appenders };
        log::set_max_level(self.max_level);
        log::set_boxed_logger(Box::new(self))?;
        Ok(handle)
    }
}

impl Log for RemoteLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        for appender in self.appenders.iter() {
            if let Err(e) = appender.append(record) {
                eprintln!("{}", e);
            }
        }
    }

    fn flush(&self) {}
}
