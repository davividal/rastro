//! What a setting is set to.

/// A setting's effective value, as text.
///
/// **Deliberately not a [`NonEmptyText`](rastro_collector::NonEmptyText).** An empty value
/// is a real state: `archive_command` is empty on a cluster that archives nothing, and
/// refusing it would turn an ordinary cluster into a failed read.
///
/// Whitespace is significant for the same reason. `log_line_prefix` ends in a space on a
/// default Debian cluster, and trimming it would report a format the server does not use.
///
/// Text rather than a number even where the value looks numeric: `shared_buffers` is
/// `16384` in units of `8kB`, and parsing it into an integer here would invite arithmetic
/// on a value whose unit lives in another field.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SettingValue(String);

impl SettingValue {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
