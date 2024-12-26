use std::ops::Deref;

use log::kv::{Error, Key, Value, VisitSource};
use serde_json::json;

type KVMap = serde_json::Map<String, serde_json::Value>;

/// Helper type to collect all key values in a log messages
#[derive(Debug, Clone)]
pub struct KeyValues(KVMap);

impl KeyValues {
    pub fn new() -> Self {
        Self(serde_json::Map::new())
    }
}

impl Default for KeyValues {
    fn default() -> Self {
        Self::new()
    }
}

// Implement `VisitSource` to collect all key value pairs into a Map
impl<'k> VisitSource<'k> for KeyValues {
    fn visit_pair(&mut self, key: Key<'k>, value: Value<'k>) -> Result<(), Error> {
        self.0.insert(key.as_str().to_owned(), json!(value));
        Ok(())
    }
}

impl Deref for KeyValues {
    type Target = KVMap;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
