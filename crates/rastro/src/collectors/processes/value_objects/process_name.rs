//! What a process calls itself.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// A process's name, as the kernel records it.
///
/// Truncated to fifteen characters, because that is what `TASK_COMM_LEN` allows, so a box
/// running Prometheus exporters reports `node_exporter` in full and `systemd_exporte`
/// cut short. rastro records the truncation rather than guessing at the rest.
///
/// **Read from `/proc/<pid>/status` rather than from `/proc/<pid>/stat`, and that is the
/// one interesting decision in this collector's source.** The `stat` file puts the name in
/// parentheses as its second field, and the name may itself contain parentheses and
/// spaces: this very box runs a process whose name is `(sd-pam)`. Splitting `stat` on
/// whitespace therefore mis-slots every field after the name, and doing it correctly means
/// finding the *last* `)` in the line. The `status` file is `Name:\tvalue`, one field per
/// line, and the trap does not exist.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProcessName(NonEmptyText);

impl ProcessName {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "process name")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&ProcessName> for Observation {
    fn from(name: &ProcessName) -> Self {
        Observation::text(name.as_str())
    }
}
