use async_trait::async_trait;
use log::{LevelFilter, Record};

use crate::error::Result;

/// Type definition of module level pair
pub type ModuleLevel = (String, LevelFilter);

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
                .find(|(m, _)| module.starts_with(m))
            {
                if &record.level().to_level_filter() > level {
                    return false;
                }
            }
        }
        true
    }
}
