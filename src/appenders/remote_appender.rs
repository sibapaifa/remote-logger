use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use log::LevelFilter;
use tokio::{
    sync::mpsc::{channel, Receiver, Sender},
    time::sleep,
};

use crate::log_message::LogMessage;
use crate::{
    error::Result,
    message_filter::{DefaultMessageFilter, MessageFilter},
};

use super::{Append, ModuleLevel};

/// Type defining message queue
/// contains `LogMessage` in VecDeque
type MessageQueue = Arc<Mutex<VecDeque<LogMessage>>>;

/// Trait to send log messages to remote target
#[async_trait]
pub trait SendLog: Send + Sync + 'static {
    const BATCH_SIZE: usize;
    const WAIT_TIMEOUT: Duration;
    async fn send_log(&self, messages: &[LogMessage]);
}

/// Appender that sends messages to remote target asynchronously
pub struct RemoteAppender<S: SendLog> {
    target: Option<String>,
    max_level: LevelFilter,
    module_levels: Vec<ModuleLevel>,
    message_queue: MessageQueue,
    log_sender: Arc<S>,
    close_notifier: Sender<()>,
    message_filter: Box<dyn MessageFilter>,
}

// Implement RemoteAppender
impl<S: SendLog> RemoteAppender<S> {
    pub fn new(log_sender: S) -> Self {
        let message_queue = Arc::new(Mutex::new(VecDeque::new()));
        let (close_notifier, receiver) = channel::<()>(1);
        let log_sender = Arc::new(log_sender);
        let remote_appender = Self {
            target: None,
            max_level: LevelFilter::Trace,
            module_levels: vec![],
            message_queue,
            log_sender,
            close_notifier,
            message_filter: Box::new(DefaultMessageFilter),
        };
        remote_appender.start_worker(receiver);
        remote_appender
    }

    async fn flush(&self) -> Result<()> {
        let batch_size = S::BATCH_SIZE;
        let messages = {
            let mut message_queue = self.message_queue.lock()?;
            message_queue.drain(0..).collect::<Vec<_>>()
        };
        let mut set = tokio::task::JoinSet::new();
        for chunk in messages.chunks(batch_size) {
            let log_sender = self.log_sender.clone();
            let chunk = chunk.to_vec();
            set.spawn(async move {
                log_sender.send_log(&chunk).await;
            });
        }
        set.join_all().await;
        Ok(())
    }

    fn start_worker(&self, mut close_notification: Receiver<()>) {
        let message_queue = self.message_queue.clone();
        let log_sender = self.log_sender.clone();
        let batch_size = S::BATCH_SIZE;
        let timeout = S::WAIT_TIMEOUT;
        tokio::spawn(async move {
            tokio::select! {
                _ = close_notification.recv() => {},
                _ = async {
                    loop {
                        sleep(timeout).await;
                        let messages = {
                            let Ok(mut message_queue) = message_queue.try_lock() else {
                                continue;
                            };
                            let size = message_queue.len();
                            let size = if size < batch_size {size} else {batch_size};
                            message_queue.drain(0..size).collect::<Vec<_>>()
                        };
                        if !messages.is_empty() {
                            log_sender.send_log(&messages).await;
                        }
                    }
                } => {}
            }
        });
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

/// Implement Append for RemoteAppender
#[async_trait]
impl<S: SendLog> Append for RemoteAppender<S> {
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
            let message: LogMessage = record.try_into()?;
            let mut message_queue = self.message_queue.lock()?;
            message_queue.push_back(message);
        }
        Ok(())
    }
    async fn close(&self) -> Result<()> {
        self.close_notifier.send(()).await?;
        self.flush().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use log::RecordBuilder;

    const EXPECTED_BATCH_SIZE: u32 = 3;
    struct LogSender(Arc<Mutex<u32>>);
    #[async_trait]
    impl SendLog for LogSender {
        const BATCH_SIZE: usize = EXPECTED_BATCH_SIZE as usize;
        const WAIT_TIMEOUT: Duration = Duration::from_millis(10);
        async fn send_log(&self, messages: &[LogMessage]) {
            assert!(messages.len().le(&Self::BATCH_SIZE));
            let mut count = self.0.lock().unwrap();
            *count += 1;
        }
    }

    #[tokio::test]
    async fn test_remote_appender() {
        let count = Arc::new(Mutex::new(0));
        let log_sender = LogSender(count.clone());
        let remote_appender = RemoteAppender::new(log_sender);
        let message_count = 10;
        for _ in 0..message_count {
            let record = RecordBuilder::new()
                .args(format_args!("test message"))
                .build();
            remote_appender.append(&record).unwrap();
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        remote_appender.close().await.unwrap();
        {
            let expected_count = (message_count / EXPECTED_BATCH_SIZE) + 1;
            let count = count.lock().unwrap();
            assert_eq!(*count, expected_count);
        }
    }

    #[tokio::test]
    async fn test_message_filter() {
        struct RemoteAppenderMessageFilter;
        impl MessageFilter for RemoteAppenderMessageFilter {
            fn filter_message(&self, record: &log::Record) -> bool {
                record.level().eq(&log::Level::Error)
            }
        }

        let count = Arc::new(Mutex::new(0));
        let log_sender = LogSender(count.clone());
        let filter = RemoteAppenderMessageFilter;
        let remote_appender = RemoteAppender::new(log_sender).with_message_filter(filter);
        let message_count = 10;
        for _ in 0..message_count {
            let record = RecordBuilder::new()
                .level(log::Level::Info)
                .args(format_args!("test message"))
                .build();
            remote_appender.append(&record).unwrap();
            let record = RecordBuilder::new()
                .level(log::Level::Error)
                .args(format_args!("test message"))
                .build();
            remote_appender.append(&record).unwrap();
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        remote_appender.close().await.unwrap();
        {
            let expected_count = (message_count / EXPECTED_BATCH_SIZE) + 1;
            let count = count.lock().unwrap();
            assert_eq!(*count, expected_count);
        }
    }
}
