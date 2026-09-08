//! What a location block matches.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// The whole of a `location` directive's arguments, modifier included: `= /health`,
/// `~* \.php$`, `^~ /static/`, or a bare prefix.
///
/// One value rather than a modifier beside a pattern, because the pair is what identifies
/// the block and what a reader compares. Splitting it would also invite the model to claim
/// nginx's matching precedence, which is a rule about requests rather than about state.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct LocationPattern(NonEmptyText);

impl LocationPattern {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "location pattern")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&LocationPattern> for Observation {
    fn from(pattern: &LocationPattern) -> Self {
        Observation::text(pattern.as_str())
    }
}
