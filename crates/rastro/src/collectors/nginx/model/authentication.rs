//! The password wall in front of a virtual host or a location.

use rastro_collector::{AbsolutePath, NonEmptyText, Observation};

use crate::collectors::nginx::model::AuthorisedUser;

/// An `auth_basic` realm and the file of users behind it.
///
/// **Absence here is the state worth catching.** A location that used to carry an
/// `auth_basic` and no longer does is a wall that came down, and nothing else in a
/// fingerprint would say so: the file of users is still on disk, still owned by root, still
/// the same bytes, and no longer consulted.
///
/// `auth_basic off;` is nginx's way of taking the wall down for one location under a server
/// that has one, and it reaches the document as the realm `off` rather than as nothing at
/// all, because the two are different configurations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Authentication {
    pub realm: Option<NonEmptyText>,
    pub user_file: Option<AbsolutePath>,
    /// Who the user file names, when there is one and it could be read.
    pub users: Vec<AuthorisedUser>,
    /// Why the user file could not be read, when it could not.
    ///
    /// Recorded rather than left as an empty list of users: a wall with nobody behind it and
    /// a wall rastro could not see behind are opposite facts, and one of them is a
    /// misconfiguration serving 403 to everybody.
    pub refusal: Option<NonEmptyText>,
}

impl From<&Authentication> for Observation {
    fn from(authentication: &Authentication) -> Self {
        Observation::object([
            (
                "realm",
                authentication
                    .realm
                    .as_ref()
                    .map_or_else(Observation::null, |realm| Observation::text(realm.as_str())),
            ),
            (
                "user_file",
                authentication
                    .user_file
                    .as_ref()
                    .map_or_else(Observation::null, |file| Observation::text(file.as_str())),
            ),
            (
                "refusal",
                authentication
                    .refusal
                    .as_ref()
                    .map_or_else(Observation::null, |refusal| {
                        Observation::text(refusal.as_str())
                    }),
            ),
            (
                "users",
                Observation::list(authentication.users.iter().map(Observation::from)),
            ),
        ])
    }
}
