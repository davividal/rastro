//! One slice of a repository's package pool.

use rastro_collector::{CollectionError, NonEmptyText};

/// A component, which is how a suite is divided by licensing or support policy.
///
/// `main`, `contrib`, `non-free`, `non-free-firmware`. Which of these a box enables is
/// a policy decision somebody made, and enabling `non-free` is exactly the kind of
/// change a fingerprint exists to catch.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Component(NonEmptyText);

impl Component {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "component")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}
