//! What came of reading one of PAM's environment sources.

/// The facet's own `ok | absent | error` vocabulary, one level down.
///
/// A source that is not on the box is **state**: a box with no `/etc/security/pam_env.conf`
/// sets no rules, which is a fact about it rather than a failure to read one. A source that
/// is there and will not open is an `error` carrying its reason, because reporting `absent`
/// for a file rastro was merely not allowed to read would be a confident lie — the same
/// distinction the three-valued `Presence` exists for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileStatus {
    Ok,
    Absent,
    Unreadable(String),
}

impl FileStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Absent => "absent",
            Self::Unreadable(_) => "error",
        }
    }

    pub fn reason(&self) -> Option<&str> {
        match self {
            Self::Unreadable(why) => Some(why),
            _ => None,
        }
    }
}
