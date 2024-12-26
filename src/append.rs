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
    const BATCH_SIZE: usize = 100;
    const WAIT_TIMEOUT: Duration = Duration::from_secs(1);
    async fn send_log(&self, messages: &[LogMessage]);
}

/// Trait implemented by different appenders
#[async_trait]
pub trait Append: Send + Sync + 'static {
    fn target(&self) -> &str;
    fn max_level(&self) -> LevelFilter;
    fn module_level(&self) -> &[ModuleLevel];
    fn append(&self, record: &log::Record) -> Result<()>;
    async fn close(&self) -> Result<()> {
        Ok(())
    }
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
                println!("{}", message);
            }
        }
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

/// Appender that bulk upload messages to remote API
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
                        log_sender.send_log(&messages).await;
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

/// Implement Append for FileAppender
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
