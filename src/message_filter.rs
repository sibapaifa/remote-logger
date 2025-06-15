pub trait MessageFilter: Send + Sync + 'static {
    /// Returns `true` for messages to be appended
    fn filter_message(&self, record: &log::Record) -> bool;
}

pub struct DefaultMessageFilter;

impl MessageFilter for DefaultMessageFilter {
    fn filter_message(&self, _record: &log::Record) -> bool {
        true
    }
}
