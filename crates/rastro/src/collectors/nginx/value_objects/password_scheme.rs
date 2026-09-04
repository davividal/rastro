//! How a basic-auth password is stored.

use rastro_collector::Observation;

/// The schemes an `htpasswd` file uses, and whether each carries a salt.
///
/// **The salt is what decides whether a digest may be recorded**, which is the same rule the
/// postgresql facet applies to a role's verifier and for the same reason. A salted verifier
/// is different on every box and every rotation, so a digest of it says only *that* it
/// changed. An unsalted one is a pure function of the password: anybody holding the document
/// could hash a guess, spell it the way the file does, digest that, and compare — turning a
/// fingerprint into an offline oracle over the passwords of everybody in the file.
///
/// Recognising the salted schemes rather than excluding the unsalted ones fails closed: a
/// scheme nobody here has heard of gets no digest until somebody has checked how it is
/// stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PasswordScheme {
    /// Apache's own salted MD5, `$apr1$`, and what `htpasswd` writes by default.
    Apr1,
    /// `$2a$`, `$2b$`, `$2y$`.
    Bcrypt,
    /// glibc's salted SHA-256 and SHA-512, `$5$` and `$6$`.
    ShaCrypt,
    /// `{SHA}`: an unsalted SHA-1 of the password itself.
    Sha1,
    /// Anything else, plaintext included. nginx accepts a plaintext password in this file.
    Unrecognised,
}

impl PasswordScheme {
    /// What the stored verifier's prefix says it is.
    pub fn of(verifier: &str) -> Self {
        match verifier {
            _ if verifier.starts_with("$apr1$") => Self::Apr1,
            _ if verifier.starts_with("$2a$")
                || verifier.starts_with("$2b$")
                || verifier.starts_with("$2y$") =>
            {
                Self::Bcrypt
            }
            _ if verifier.starts_with("$5$") || verifier.starts_with("$6$") => Self::ShaCrypt,
            _ if verifier.starts_with("{SHA}") => Self::Sha1,
            _ => Self::Unrecognised,
        }
    }

    /// Whether a digest of the verifier may be recorded.
    pub fn is_salted(&self) -> bool {
        match self {
            Self::Apr1 | Self::Bcrypt | Self::ShaCrypt => true,
            Self::Sha1 | Self::Unrecognised => false,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Apr1 => "apr1",
            Self::Bcrypt => "bcrypt",
            Self::ShaCrypt => "sha-crypt",
            Self::Sha1 => "sha1",
            Self::Unrecognised => "unrecognised",
        }
    }
}

impl From<&PasswordScheme> for Observation {
    fn from(scheme: &PasswordScheme) -> Self {
        Observation::text(scheme.as_str())
    }
}
