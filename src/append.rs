use std::{
    collections::VecDeque,
    fs::File,
    io::{BufWriter, Write},
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use log::{LevelFilter, Record};
use tokio::{
    sync::mpsc::{channel, Receiver, Sender},
    time::sleep,
};

use crate::{
    error::Result,
    log_message::{DefaultFormatter, FormatMessage, LogMessage},
};

/// Type definition of module level pair
type ModuleLevel = (String, LevelFilter);

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

/// Trait implemented by different appenders
#[async_trait]
pub trait Append: Send + Sync + 'static {
    fn target(&self) -> &str;
    fn max_level(&self) -> LevelFilter;
    fn module_level(&self) -> &[ModuleLevel];
    fn append(&self, record: &log::Record) -> Result<()>;
    async fn close(&self) -> Result<()>;
    fn enabled(&self, record: &Record) -> bool {
        if record.level().to_level_filter() > self.max_level() {
            return false;
        }
        let target = record.target();
        let module = record.module_path().unwrap_or_default();
        // When self.target is empty then receive all message
        // By default target field is populated with module_path value
        // If target is equal to module_path then receive message
        // Otherwise split target value by semi-colon(;) and check
        // if it matches with self.target
        if !self.target().is_empty()
            && target != module
            && target.split(';').all(|s| s != self.target())
        {
            return false;
        }
        // Check module wise levels
        if !module.is_empty() {
            if let Some((_, level)) = self
                .module_level()
                .iter()
                .find(|(m, _)| m.starts_with(module))
            {
                if &record.level().to_level_filter() > level {
                    return false;
                }
            }
        }
        true
    }
}

/// Appender that append messages to stdout
pub struct ConsoleAppender {
    target: Option<String>,
    max_level: LevelFilter,
    module_levels: Vec<ModuleLevel>,
    formatter: Box<dyn FormatMessage>,
}

/// Implement ConsoleAppender
impl ConsoleAppender {
    pub fn new() -> Self {
        Self {
            target: None,
            max_level: LevelFilter::Trace,
            module_levels: vec![],
            formatter: Box::new(DefaultFormatter),
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
        if self.enabled(record) {
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

/// Appender that append messages to file
pub struct FileAppender {
    target: Option<String>,
    max_level: LevelFilter,
    module_levels: Vec<ModuleLevel>,
    file: Mutex<BufWriter<File>>,
    formatter: Box<dyn FormatMessage>,
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
        Ok(Self {
            target: None,
            max_level: LevelFilter::Trace,
            module_levels: vec![],
            file,
            formatter,
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
        if self.enabled(record) {
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

type StdSender = std::sync::mpsc::Sender<LogMessage>;
/// Appender that pass on the message to an mpsc channel
pub struct ChannelAppender {
    target: Option<String>,
    max_level: LevelFilter,
    module_levels: Vec<ModuleLevel>,
    sender: StdSender,
}

impl ChannelAppender {
    pub fn new(sender: StdSender) -> Self {
        Self {
            target: None,
            max_level: LevelFilter::Trace,
            module_levels: vec![],
            sender,
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
        self.sender.send(record.try_into()?)?;
        Ok(())
    }
    async fn close(&self) -> Result<()> {
        Ok(())
    }
}

/// Appender that sends messages to remote target asynchronously
pub struct RemoteAppender<S: SendLog> {
    target: Option<String>,
    max_level: LevelFilter,
    module_levels: Vec<ModuleLevel>,
    message_queue: MessageQueue,
    log_sender: Arc<S>,
    close_notifier: Sender<()>,
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
        if self.enabled(record) {
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

    #[tokio::test]
    async fn test_remote_appender() {
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
}
