use async_trait::async_trait;
use log::LevelFilter;

use crate::{
    error::Result,
    log_message::LogMessage,
    message_filter::{DefaultMessageFilter, MessageFilter},
};

use super::{Append, ModuleLevel};

type StdSender = std::sync::mpsc::Sender<LogMessage>;
/// Appender that pass on the message to an mpsc channel
pub struct ChannelAppender {
    target: Option<String>,
    max_level: LevelFilter,
    module_levels: Vec<ModuleLevel>,
    sender: StdSender,
    message_filter: Box<dyn MessageFilter>,
}

impl ChannelAppender {
    pub fn new(sender: StdSender) -> Self {
        Self {
            target: None,
            max_level: LevelFilter::Trace,
            module_levels: vec![],
            sender,
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

    pub fn with_module_level(mut self, module_level: ModuleLevel) -> Self {
        let module_levels: &mut Vec<ModuleLevel> = self.module_levels.as_mut();
        module_levels.push(module_level);
        module_levels.sort_by_key(|l| l.0.len().wrapping_neg());
        self
    }

    pub fn with_message_filter<F: MessageFilter>(mut self, message_filter: F) -> Self {
        self.message_filter = Box::new(message_filter);
        self
    }
}

/// Implement `Append` for ChannelAppender
#[async_trait]
impl Append for ChannelAppender {
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
            self.sender.send(record.try_into()?)?;
        }
        Ok(())
    }
    async fn close(&self) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    };

    use super::*;
    use log::RecordBuilder;

    #[test]
    fn test_channel_appender() {
        let (sender, receiver) = std::sync::mpsc::channel::<LogMessage>();
        let channel_appender = ChannelAppender::new(sender);
        let record = RecordBuilder::new()
            .target("test_target")
            .module_path(Some("module"))
            .level(log::Level::Info)
            .build();
        channel_appender.append(&record).unwrap();
        let msg = receiver.recv().unwrap();
        assert_eq!(msg.level(), log::Level::Info);
    }

    #[test]
    fn test_message_filter() {
        struct Filter;
        impl MessageFilter for Filter {
            fn filter_message(&self, record: &log::Record) -> bool {
                record.level() == log::Level::Warn
            }
        }

        let counter = Arc::new(AtomicU32::new(0));
        let (sender, receiver) = std::sync::mpsc::channel::<LogMessage>();
        let filter = Filter;
        let channel_appender = ChannelAppender::new(sender).with_message_filter(filter);
        let count = 10;
        let counter_copy = counter.clone();
        let handle = std::thread::spawn(move || {
            while receiver.recv().is_ok() {
                counter_copy.fetch_add(1, Ordering::SeqCst);
            }
        });
        for _ in 0..count {
            let record = RecordBuilder::new()
                .args(format_args!("test message"))
                .level(log::Level::Info)
                .build();
            channel_appender.append(&record).unwrap();
            let record = RecordBuilder::new()
                .args(format_args!("test message"))
                .level(log::Level::Warn)
                .build();
            channel_appender.append(&record).unwrap();
            let record = RecordBuilder::new()
                .args(format_args!("test message"))
                .level(log::Level::Error)
                .build();
            channel_appender.append(&record).unwrap();
        }
        drop(channel_appender);
        handle.join().unwrap();
        let value = counter.load(Ordering::SeqCst);
        assert_eq!(value, count);
    }
}
