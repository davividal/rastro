//! The account a container's process runs as.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// The account exactly as the container was configured with it: `appuser`, `1000`,
/// `1000:1000`.
///
/// **Not resolved to a name or a number, and that is not laziness.** The passwd file that
/// would turn `appuser` into a uid is the *container's* own, inside an image this collector
/// does not open files in, and the host's passwd would answer a different question. So the
/// spelling is what is recorded, and whether it changed is what a diff can honestly say.
///
/// Absent rather than empty where the image decides: see [`crate::collectors::containers`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ContainerAccount(NonEmptyText);

impl ContainerAccount {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "container account")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&ContainerAccount> for Observation {
    fn from(account: &ContainerAccount) -> Self {
        Observation::text(account.as_str())
    }
}
