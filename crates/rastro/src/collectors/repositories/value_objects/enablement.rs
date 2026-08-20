//! Whether a configured repository is actually in use.

use rastro_collector::Observation;

/// Whether an entry is live or switched off.
///
/// **Disabled entries are recorded, not skipped, and that is the whole reason this
/// type exists.** A commented-out repository is the single most common shape of
/// "somebody changed where this box gets packages from": the old line is left in place
/// with a `#` in front of it and a new one added underneath. A collector that dropped
/// comments would show that as one repository appearing, when what happened was one
/// being swapped for another.
///
/// Two spellings reach this, and they mean the same thing. A one-line entry is
/// disabled by commenting it out, and a deb822 paragraph by `Enabled: no`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Enablement {
    Enabled,
    Disabled,
}

impl Enablement {
    /// Reads a deb822 `Enabled:` field, which apt treats as a boolean.
    ///
    /// apt accepts `yes`/`no`, `true`/`false` and `1`/`0`, case-insensitively, and
    /// anything it does not recognise it treats as enabled while warning. rastro
    /// follows that rather than refusing, because the host's behaviour is what a
    /// fingerprint has to describe: a repository apt is using is in use whatever the
    /// field says.
    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "no" | "false" | "0" => Self::Disabled,
            _ => Self::Enabled,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Enabled => "enabled",
            Self::Disabled => "disabled",
        }
    }
}

impl From<&Enablement> for Observation {
    fn from(enablement: &Enablement) -> Self {
        Observation::text(enablement.as_str())
    }
}
