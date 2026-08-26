//! What a flag was set to.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// The value given to a flag: `8080`, `0.0.0.0:9100`, `10s`, `true`.
///
/// **Kept as the text the unit carried, never converted.** `--port=8080` is a number,
/// `--housekeeping_interval=10s` is a duration and `--store_container_labels=true` is a
/// boolean, and the agent is the only thing that knows which is which. Parsing them here
/// would mean rastro deciding a flag's type from the shape of one value, and being wrong
/// the first time a duration is written `10000ms`.
///
/// The endpoint is the one exception, and it is deliberate: a listen address is read into
/// a host and a port *as well as* being kept here, because comparing it against what is
/// actually bound is the whole reason an operator reads this facet.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SettingValue(NonEmptyText);

impl SettingValue {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "flag value")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&SettingValue> for Observation {
    fn from(value: &SettingValue) -> Self {
        Observation::text(value.as_str())
    }
}
