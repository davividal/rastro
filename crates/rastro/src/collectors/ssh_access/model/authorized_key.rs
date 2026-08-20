//! One key that may log in.

use rastro_collector::Observation;

use crate::collectors::ssh_access::value_objects::{KeyComment, KeyOption, KeyType, PublicKey};

/// A key from an `authorized_keys` file.
///
/// **The three fields answer three different questions, and a change to any one of them is a
/// change to who can do what.** The key says *who*, the options say *what they may do*, and
/// the comment says who somebody believed it was.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct AuthorizedKey {
    pub key_type: KeyType,
    pub key: PublicKey,
    pub comment: KeyComment,
    /// Sorted, because the order OpenSSH accepts them in carries no meaning and reordering a
    /// line must not read as a change. Empty is the common case and means the key is
    /// unrestricted.
    pub options: Vec<KeyOption>,
}

impl From<&AuthorizedKey> for Observation {
    fn from(key: &AuthorizedKey) -> Self {
        Observation::object([
            ("comment", Observation::from(&key.comment)),
            ("key", Observation::from(&key.key)),
            ("key_type", Observation::from(&key.key_type)),
            (
                "options",
                Observation::list(key.options.iter().map(Observation::from)),
            ),
            (
                // Not derived at render time for convenience: whether a key is restricted at
                // all is the question an auditor asks first, and burying it in the length of a
                // list makes it something they have to work out.
                "restricted",
                Observation::boolean(!key.options.is_empty()),
            ),
        ])
    }
}
