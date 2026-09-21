//! One account on a node, and the verifier it is not allowed to print.

use rastro_collector::Observation;

use crate::collectors::rabbitmq::value_objects::PasswordHashing;

/// A user as the definitions export describes it.
///
/// **The verifier is carried and marked rather than left behind**, which is the opposite of
/// what the `postgresql` facet does and not a change of mind. There the digest is taken by
/// the server and the material never enters rastro; here the CLI hands over the whole
/// document, so the material arrives whatever rastro would prefer, and the choice left is
/// what to do with it. Marking it `sensitive` puts a stand-in in the default document and the
/// value itself behind `--raw`, which is exactly the mechanism the design describes and the
/// one thing `postgresql` cannot use.
///
/// [`PasswordHashing::carries_verifier`] decides whether it is carried at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User {
    pub tags: Vec<String>,

    /// The scheme the verifier was produced under, always recorded.
    ///
    /// Beside the verifier because the two say different things: a scheme that changed is
    /// drift in how the box stores credentials, and a verifier that changed is a rotation.
    /// One field could not report both.
    pub password_hashing: Option<PasswordHashing>,

    /// The stored verifier, where its scheme is one rastro has read.
    ///
    /// Absent means one of two things and the field beside it says which: a user with no
    /// password at all, or a verifier whose scheme withholds it.
    pub password_hash: Option<String>,

    /// The per-user limits, such as a maximum number of connections or channels.
    pub limits: Vec<UserLimit>,
}

/// One limit a user was given, as a name and a number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserLimit {
    pub name: String,
    pub value: i64,
}

impl From<&User> for Observation {
    fn from(user: &User) -> Self {
        Observation::object([
            (
                "tags",
                Observation::list(user.tags.iter().map(|tag| Observation::text(tag.as_str()))),
            ),
            (
                "password_hashing",
                match &user.password_hashing {
                    Some(hashing) => Observation::text(hashing.as_str()),
                    None => Observation::null(),
                },
            ),
            (
                "password_hash",
                match &user.password_hash {
                    // The one call in this facet that decides whether a document may print a
                    // credential, and the annotation is the whole of the mechanism.
                    Some(verifier) => Observation::text(verifier.as_str()).sensitive(),
                    None => Observation::null(),
                },
            ),
            (
                "limits",
                Observation::object(
                    user.limits
                        .iter()
                        .map(|limit| (limit.name.as_str(), Observation::integer(limit.value))),
                ),
            ),
        ])
    }
}
