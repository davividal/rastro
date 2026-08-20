//! What a key says about itself.

use rastro_collector::Observation;

/// The free text at the end of an `authorized_keys` line.
///
/// Conventionally `user@host`, which `ssh-keygen` puts there, and often an email address or a
/// note about which laptop the key lives on. It is the only field that says *whose* key this
/// is in human terms, and nothing enforces that it says anything true.
///
/// **Deliberately not empty-checked**, because a key with no comment is ordinary: anything
/// generated with `-C ""` or pasted by hand has none. Empty is therefore a value rather than
/// a misread, which is why this is not built on a non-empty text type.
///
/// The comment is the rest of the line, spaces and all: OpenSSH does not tokenise it, so a
/// comment reading `Adam's laptop, 2024` is one comment and not three fields.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct KeyComment(String);

impl KeyComment {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&KeyComment> for Observation {
    fn from(comment: &KeyComment) -> Self {
        Observation::text(comment.as_str())
    }
}
