use std::io::Write;

use async_trait::async_trait;
use log::LevelFilter;

use crate::{
    error::Result,
    log_message::{DefaultFormatter, FormatMessage},
    message_filter::{DefaultMessageFilter, MessageFilter},
};

use super::{Append, ModuleLevel};

/// Appender that append messages to stdout
pub struct ConsoleAppender {
    target: Option<String>,
    max_level: LevelFilter,
    module_levels: Vec<ModuleLevel>,
    formatter: Box<dyn FormatMessage>,
    message_filter: Box<dyn MessageFilter>,
}

/// Implement ConsoleAppender
impl ConsoleAppender {
    pub fn new() -> Self {
        Self {
            target: None,
            max_level: LevelFilter::Trace,
            module_levels: vec![],
            formatter: Box::new(DefaultFormatter),
            message_filter: Box::new(DefaultMessageFilter),
        }
    }

    pub fn with_target(mut self, target: &str) -> Self {
        self.target = Some(target.to_owned());
        self
    }

    pub fn with_max_level(mut self, level: LevelFilter) -> Self {
        self.max_level = level;
        self
    }

    pub fn with_formatter<F: FormatMessage>(mut self, formatter: F) -> Self {
        self.formatter = Box::new(formatter);
        self
    }

    pub fn with_message_filter<F: MessageFilter>(mut self, message_filter: F) -> Self {
        self.message_filter = Box::new(message_filter);
        self
    }

    pub fn with_module_level(mut self, module_level: ModuleLevel) -> Self {
        let module_levels: &mut Vec<ModuleLevel> = self.module_levels.as_mut();
        module_levels.push(module_level);
        module_levels.sort_by_key(|l| l.0.len().wrapping_neg());
        self
    }
}

impl Default for ConsoleAppender {
    fn default() -> Self {
        Self::new()
    }
}

/// Implement Append for ConsoleAppender
#[async_trait]
impl Append for ConsoleAppender {
    fn target(&self) -> &str {
        self.target.as_deref().unwrap_or_default()
    }

    fn max_level(&self) -> LevelFilter {
        self.max_level
    }

    fn module_level(&self) -> &[ModuleLevel] {
        self.module_levels.as_slice()
    }

    fn append(&self, record: &log::Record) -> Result<()> {
        if self.message_filter.filter_message(record) && self.enabled(record) {
            if let Some(message) = self.formatter.format_message(record) {
                std::io::stdout().write_all(message.as_bytes())?;
            }
        }
        Ok(())
    }

    async fn close(&self) -> Result<()> {
        std::io::stdout().flush()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use log::RecordBuilder;

    #[test]
    fn test_console_appender() {
        let test_target = "TEST_TARGET";
        let module = module_path!(); // remote_logger::append::tests
        let console_appender = ConsoleAppender::new()
            .with_max_level(LevelFilter::Debug)
            .with_target(test_target)
            .with_module_level((module.to_owned(), LevelFilter::Info));
        assert_eq!(console_appender.target, Some(test_target.to_owned()));
        assert_eq!(console_appender.max_level, LevelFilter::Debug);
        assert!(!console_appender.module_levels.is_empty());

        let record = RecordBuilder::new()
            .level(LevelFilter::Trace.to_level().unwrap())
            .build();
        let is_enabled = console_appender.enabled(&record);
        assert!(!is_enabled);

        let file = file!();
        let line = line!();
        let level = LevelFilter::Info.to_level().unwrap();
        let msg = "TEST LOG MESSAGE";
        let record = RecordBuilder::new()
            .target(test_target)
            .module_path(Some(module))
            .file(Some(file))
            .line(Some(line))
            .level(level)
            .args(format_args!("TEST LOG MESSAGE"))
            .build();
        let is_enabled = console_appender.enabled(&record);
        assert!(is_enabled);
        let message = console_appender.formatter.format_message(&record).unwrap();
        assert!(message.contains(module));
        assert!(message.contains(file));
        {
            let line = line.to_string();
            let level = level.to_string();
            assert!(message.contains(line.as_str()));
            assert!(message.contains(level.as_str()));
        }
        assert!(message.contains(msg));
    }

    #[test]
    fn test_module_level() {
        let appender = ConsoleAppender::new()
            .with_max_level(LevelFilter::Trace)
            .with_module_level(("hyper_util".to_string(), LevelFilter::Off));
        let record = RecordBuilder::new()
            .level(log::Level::Info)
            .module_path(Some("hyper_util::client::legacy::pool"))
            .build();
        assert!(!appender.enabled(&record));
    }
}
