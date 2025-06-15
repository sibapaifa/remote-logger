use std::io::Write;
use std::{fs::File, io::BufWriter, path::Path, sync::Mutex};

use async_trait::async_trait;
use log::LevelFilter;

use crate::error::Result;
use crate::log_message::{DefaultFormatter, FormatMessage};
use crate::message_filter::{DefaultMessageFilter, MessageFilter};

use super::{Append, ModuleLevel};

/// Appender that append messages to file
pub struct FileAppender {
    target: Option<String>,
    max_level: LevelFilter,
    module_levels: Vec<ModuleLevel>,
    file: Mutex<BufWriter<File>>,
    formatter: Box<dyn FormatMessage>,
    message_filter: Box<dyn MessageFilter>,
}

// Implement FileAppender
impl FileAppender {
    pub fn new<P>(file_path: P) -> Result<Self>
    where
        P: AsRef<Path>,
    {
        let file = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(file_path)?;
        let file = BufWriter::with_capacity(1024, file);
        let file = Mutex::new(file);
        let formatter = Box::new(DefaultFormatter);
        let message_filter = Box::new(DefaultMessageFilter);
        Ok(Self {
            target: None,
            max_level: LevelFilter::Trace,
            module_levels: vec![],
            file,
            formatter,
            message_filter,
        })
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

/// Implement Append for FileAppender
#[async_trait]
impl Append for FileAppender {
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
                let mut file = self.file.lock()?;
                file.write_all(message.as_bytes())?;
                file.flush()?;
            }
        }
        Ok(())
    }
    async fn close(&self) -> Result<()> {
        let mut file = self.file.lock()?;
        file.flush()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use log::RecordBuilder;

    #[test]
    fn test_file_appender() {
        let temp_file = tempfile::NamedTempFile::new().unwrap();
        let test_target = "TEST_TARGET";
        let module = module_path!(); // remote_logger::append::tests
        let file_path = temp_file.path();
        let file_appender = FileAppender::new(file_path)
            .unwrap()
            .with_target(test_target)
            .with_max_level(LevelFilter::Info);
        let record = RecordBuilder::new()
            .target(test_target)
            .module_path(Some(module))
            .level(log::Level::Info)
            .build();
        assert!(file_appender.enabled(&record));

        let file_appender = FileAppender::new(file_path)
            .unwrap()
            .with_target(test_target)
            .with_max_level(LevelFilter::Info)
            .with_module_level((module.to_owned(), LevelFilter::Error));

        let record = RecordBuilder::new()
            .target(test_target)
            .module_path(Some(module))
            .level(log::Level::Info)
            .build();
        assert!(!file_appender.enabled(&record));

        let record = RecordBuilder::new()
            .target(test_target)
            .module_path(Some(module))
            .level(log::Level::Error)
            .build();
        assert!(file_appender.enabled(&record));
    }
}
