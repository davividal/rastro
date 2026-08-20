//! What a kernel parameter is set to.

use rastro_collector::Observation;

/// One parameter's value, or the fact that the kernel would not report one.
///
/// **Deliberately not [`NonEmptyText`](rastro_collector::NonEmptyText).** An
/// empty value is ordinary and meaningful here:
/// `net.ipv4.ip_local_reserved_ports` reads back as nothing at all on a box that
/// has reserved no ports, and refusing that would lose a real setting. So the
/// only distinction this type draws is between a value the kernel gave and one it
/// declined to give.
///
/// The two are kept apart because they diff differently. An empty value renders
/// as `""` and a declined one as `null`, so setting a previously unset parameter
/// shows up as `null` becoming a value rather than as one string becoming
/// another.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SysctlValue {
    Reported(String),
    /// The parameter is readable, and reading it still produced nothing.
    ///
    /// The case this exists for is an unset `stable_secret`: the kernel's own
    /// handler answers `EIO` until a secret is configured, on a file whose mode
    /// says `0600`. That is not a failure to collect and not an absent
    /// parameter, it is the state of a parameter that has never been set, and it
    /// is what makes setting one visible in a diff.
    Withheld,
}

/// How many trailing newlines the kernel writes after a value.
///
/// Exactly one, from `proc_dostring` and every handler that follows it, and it is
/// punctuation of the interface rather than part of the value. Internal newlines
/// are a different matter and are kept: `fs.binfmt_misc.*` genuinely spans six
/// lines, one per field of a registered format, and trimming them all would fuse
/// six facts into one unreadable token.
const TRAILING_NEWLINES: usize = 1;

impl SysctlValue {
    /// The value as the kernel wrote it, less the newline that ends it.
    ///
    /// Only the last newline goes, and only if there is one. A value that is
    /// itself blank stays blank rather than becoming absent.
    pub fn reported(written: impl Into<String>) -> Self {
        let mut written = written.into();

        if written.ends_with('\n') {
            written.truncate(written.len() - TRAILING_NEWLINES);
        }

        Self::Reported(written)
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Reported(value) => Some(value),
            Self::Withheld => None,
        }
    }
}

impl From<&SysctlValue> for Observation {
    fn from(value: &SysctlValue) -> Self {
        match value {
            SysctlValue::Reported(value) => Observation::text(value.clone()),
            SysctlValue::Withheld => Observation::null(),
        }
    }
}
