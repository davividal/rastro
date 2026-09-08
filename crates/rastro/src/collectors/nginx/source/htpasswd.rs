//! The `htpasswd` file format, as nginx's basic-auth module reads it.
//!
//! One `user:verifier` per line, `#` for a comment, blank lines ignored. What the verifier
//! looks like depends on which tool wrote it, and
//! [`PasswordScheme`] is where that
//! is decided — including whether a digest of it may be recorded at all.

use std::fs;
use std::path::Path;

use rastro_collector::{CollectionError, NonEmptyText, Xxh3Digest};
use sha2::{Digest, Sha256};

use crate::collectors::nginx::model::AuthorisedUser;
use crate::collectors::nginx::value_objects::PasswordScheme;

const COMMENT: char = '#';
const FIELD_SEPARATOR: char = ':';

/// The stand-in for a value the document withholds: XXH3 over the lowercase sha256 hex.
///
/// **One recipe for one concept, and this is the one the document already uses.** The
/// postgresql facet digests a role's password the same way — its sha256 is computed by the
/// server, so the verifier is never read into the process — and the renderer's own redaction
/// takes the same two stages in the same order. A second spelling here would put two
/// different functions of "a withheld password" in one document, which is exactly what
/// having a shared digest type is meant to prevent.
fn withheld(verifier: &str) -> Xxh3Digest {
    let sha256 = Sha256::digest(verifier.as_bytes());
    let hex = sha256
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();

    Xxh3Digest::of(hex.as_bytes())
}

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
                true => Some(withheld(verifier)),
                false => None,
            },
        });
    }

    Ok(users)
}
