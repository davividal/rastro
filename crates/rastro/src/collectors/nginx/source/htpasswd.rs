//! The `htpasswd` file format, as nginx's basic-auth module reads it.
//!
//! One `user:verifier` per line, `#` for a comment, blank lines ignored. What the verifier
//! looks like depends on which tool wrote it, and
//! [`PasswordScheme`](crate::collectors::nginx::value_objects::PasswordScheme) is where that
//! is decided — including whether a digest of it may be recorded at all.

use std::fs;
use std::path::Path;

use rastro_collector::{CollectionError, NonEmptyText, Xxh3Digest};

use crate::collectors::nginx::model::AuthorisedUser;
use crate::collectors::nginx::value_objects::PasswordScheme;

const COMMENT: char = '#';
const FIELD_SEPARATOR: char = ':';

/// Reads the file a location's `auth_basic_user_file` names.
pub fn read(path: &Path) -> Result<Vec<AuthorisedUser>, CollectionError> {
    let text = fs::read_to_string(path).map_err(|error| {
        CollectionError::new(format!(
            "the user file {} could not be read: {error}",
            path.display()
        ))
    })?;

    parse(&text)
}

/// Reads the text into the users it names.
///
/// **The verifier never leaves this function.** What comes out is the scheme it is stored
/// under and, where that scheme salts it, a digest — so a password that rotated shows as a
/// changed digest and a password that did not is unreadable either way.
pub fn parse(text: &str) -> Result<Vec<AuthorisedUser>, CollectionError> {
    let mut users = Vec::new();

    for (number, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(COMMENT) {
            continue;
        }

        let (name, verifier) = line.split_once(FIELD_SEPARATOR).ok_or_else(|| {
            CollectionError::new(format!(
                "line {} of the user file holds no {FIELD_SEPARATOR:?}, so it names no user",
                number + 1
            ))
        })?;

        let scheme = PasswordScheme::of(verifier);
        users.push(AuthorisedUser {
            name: NonEmptyText::new(name, "basic-auth user name")?,
            scheme,
            digest: match scheme.is_salted() {
                true => Some(Xxh3Digest::of(verifier.as_bytes())),
                false => None,
            },
        });
    }

    Ok(users)
}
