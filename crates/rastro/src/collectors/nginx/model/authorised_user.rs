//! One account named in a basic-auth user file.

use rastro_collector::{NonEmptyText, Observation, Xxh3Digest};

use crate::collectors::nginx::value_objects::PasswordScheme;

/// A user who may pass the wall, and how their password is stored.
///
/// **The name is in the document and the password never is**, not even hashed, unless the
/// scheme salts it — see [`PasswordScheme`]. What the digest buys where it is allowed is the
/// one question a fingerprint should answer about a credential: did it change between these
/// two runs. Who was added and who was removed is answered by the names alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorisedUser {
    pub name: NonEmptyText,
    pub scheme: PasswordScheme,
    pub digest: Option<Xxh3Digest>,
}

impl From<&AuthorisedUser> for Observation {
    fn from(user: &AuthorisedUser) -> Self {
        Observation::object([
            (
                "digest",
                user.digest
                    .as_ref()
                    .map_or_else(Observation::null, Observation::from),
            ),
            ("name", Observation::text(user.name.as_str())),
            ("scheme", Observation::from(&user.scheme)),
        ])
    }
}
