use log::{Level, Record};

use crate::kvs::KeyValues;

/// Type definition for log message
#[derive(Debug, Clone)]
pub struct LogMessage {
    level: Level,
    target: String,
    module: String,
    file_name: String,
    line_no: u32,
    message: String,
    key_values: KeyValues,
}

impl LogMessage {
    pub fn level(&self) -> Level {
        self.level
    }
    pub fn target(&self) -> &str {
        &self.target
    }
    pub fn module(&self) -> &str {
        self.module.as_str()
    }
    pub fn file_name(&self) -> &str {
        self.file_name.as_str()
    }
    pub fn line_no(&self) -> u32 {
        self.line_no
    }
    pub fn message(&self) -> &str {
        self.message.as_str()
    }
    pub fn key_values(&self) -> &KeyValues {
        &self.key_values
    }
}

// Convert from `log::Record` to `LogMessage`
impl<'a> TryFrom<&Record<'a>> for LogMessage {
    type Error = log::kv::Error;

    fn try_from(value: &Record) -> Result<Self, Self::Error> {
        let level = value.level();
        let target = value.target().to_owned();
        let module = value
            .module_path()
            .map(|s| s.to_owned())
            .unwrap_or_default();
        let file_name = value.file().map(|s| s.to_owned()).unwrap_or_default();
        let line_no = value.line().unwrap_or_default();
        let message = format!("{}", value.args());
        let mut key_values = KeyValues::new();
        value.key_values().visit(&mut key_values)?;
        Ok(Self {
            level,
            target,
            module,
            file_name,
            line_no,
            message,
            key_values,
        })
    }
}

/// Trait to format `log::Record` into a message string
pub trait FormatMessage: Send + Sync + 'static {
    fn format_message(&self, record: &Record) -> Option<String>;
}

/// Default implementor of FormatMessage trait
pub struct DefaultFormatter;

impl FormatMessage for DefaultFormatter {
    fn format_message(&self, record: &Record) -> Option<String> {
        let timestamp = get_timestamp();
        let module = record.module_path().unwrap_or_default();
        let file = record.file().unwrap_or_default();
        let line = record.line().unwrap_or_default();
        let level = record.level();
        let args = record.args();
        let message = format!(
            "{}{:<5} {} [{}:{}] {}",
            timestamp, level, module, file, line, args
        );
        Some(message)
    }
}

#[cfg(feature = "chrono-utc")]
fn get_timestamp() -> String {
    chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string()
}

#[cfg(feature = "chrono-local")]
fn get_timestamp() -> String {
    chrono::Local::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string()
}

#[cfg(all(not(feature = "chrono-local"), not(feature = "chrono-utc")))]
fn get_timestamp() -> String {
    String::new()
}
