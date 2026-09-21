//! What one user may do in one vhost.

use rastro_collector::Observation;

/// A user's permissions in a vhost, as three patterns.
///
/// **The patterns are kept as the text they are.** Each is a regular expression the broker
/// compiles against resource names, and rastro neither compiles nor normalises one: what a
/// fingerprint answers is whether the text changed, and a normalised pattern would hide an
/// edit that happened to mean the same thing. It is still an edit somebody made.
///
/// The three are separate because they are separate grants: `configure` is the right to
/// declare and delete, `write` to publish and bind, `read` to consume and unbind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Permission {
    pub configure: String,
    pub write: String,
    pub read: String,
}

impl From<&Permission> for Observation {
    fn from(permission: &Permission) -> Self {
        Observation::object([
            (
                "configure",
                Observation::text(permission.configure.as_str()),
            ),
            ("write", Observation::text(permission.write.as_str())),
            ("read", Observation::text(permission.read.as_str())),
        ])
    }
}
