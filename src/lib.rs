use std::sync::Arc;

use log::{LevelFilter, Log};

use crate::{appenders::Append, error::Result};

mod appenders;
mod error;
mod kvs;
mod log_message;
mod message_filter;

pub use crate::appenders::ChannelAppender;
pub use crate::appenders::ConsoleAppender;
pub use crate::appenders::FileAppender;
pub use crate::appenders::RemoteAppender;
pub use crate::appenders::SendLog;
pub use crate::log_message::FormatMessage;
pub use crate::log_message::LogMessage;

type AppendersList = Arc<Vec<Box<dyn Append>>>;

/// Provides `close_all` method
/// Which ensures all in-flight messages are flushed.
/// `close_all` method must be called before shutdown.
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

/// Implements [`Log`] and initlizes logger.
/// `init` method must be called to start logging.
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
