//! What a unit is called.

use rastro_collector::{CollectionError, NonEmptyText};

/// A unit's name, exactly as systemd spells it.
///
/// **The escaping is left alone, and that is deliberate.** systemd encodes a `-` inside
/// a path component as `\x2d`, so a device unit arrives as
/// `dev-disk-by\x2ddiskseq-1.device`. Decoding it to `dev-disk-by-diskseq-1` would
/// destroy the only thing that distinguishes a separator from a literal, and the
/// escaped form is the real name: it is what `systemctl status` takes and what the unit
/// is called on disk.
///
/// The suffix carries the unit's type, and a name ending `@.service` is a *template*
/// rather than a unit: it is never loaded itself, only instantiated.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UnitName(NonEmptyText);

/// The suffix of a transient login session's scope.
const SESSION_SCOPE_SUFFIX: &str = ".scope";

/// The prefix systemd gives a login session's scope.
const SESSION_SCOPE_PREFIX: &str = "session-";

impl UnitName {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "unit name")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Whether this unit exists only for as long as somebody is logged in.
    ///
    /// **The one piece of churn in this facet that changes a *key* rather than a
    /// value, which is why it needs catching.** systemd names a login session's scope
    /// `session-779.scope`, and the number is a counter that increases with every
    /// login, rastro's own SSH session included. Two runs inside one session agree, so
    /// this does not break the determinism harness; two runs either side of a logout
    /// disagree about a key, which is exactly the noise floor that teaches an operator
    /// the tool is noisy.
    ///
    /// Only the counter-bearing scope is treated this way. `user@1000.service`,
    /// `user-1000.slice` and `run-user-1000.mount` all embed a uid rather than a
    /// counter, and a uid appearing is a real change worth seeing.
    pub fn is_a_login_session(&self) -> bool {
        let name = self.as_str();

        match name
            .strip_prefix(SESSION_SCOPE_PREFIX)
            .and_then(|rest| rest.strip_suffix(SESSION_SCOPE_SUFFIX))
        {
            Some(counter) => !counter.is_empty() && counter.bytes().all(|b| b.is_ascii_digit()),
            None => false,
        }
    }
}
