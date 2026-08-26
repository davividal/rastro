//! One flag, named without its dashes.

use rastro_collector::{CollectionError, NonEmptyText};

/// The name of a flag an agent was started with: `port`, `web.listen-address`.
///
/// **The dashes are stripped, and that is a normalisation rather than a loss.** Go's flag
/// package accepts `-port` and `--port` as the same flag, so keeping the prefix would let a
/// unit rewritten from one spelling to the other read as a changed setting when nothing
/// about the deployment moved.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SettingName(NonEmptyText);

impl SettingName {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "flag name")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}
