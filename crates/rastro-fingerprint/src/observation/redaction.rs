//! What stands in for a value the document must not print.

use sha2::{Digest, Sha256};

use crate::digest::Xxh3Digest;
use crate::observation::scalar::Scalar;

/// Names the substitution and the recipe, so no reader mistakes a stand-in for the value.
///
/// The algorithm is in the document rather than only in the documentation, because a bare
/// digest on a field whose name is not obviously a secret reads exactly like a value.
pub const REDACTION_PREFIX: &str = "redacted:sha256+xxh3:";

/// The digest that stands in for `scalar`, or nothing where there is no value to withhold.
///
/// **Two stages, and the order is the contract.** sha256 of the material, rendered as
/// lowercase hex, then [`Xxh3Digest`] over those hex characters. Neither stage is
/// decoration:
///
/// - The sha256 is what makes the stand-in defensible for a *secret*. Inverting the pair
///   costs one sha256 per guess, so the work factor is sha256's and not XXH3-64's, which a
///   modern machine inverts by brute force in seconds over any small space.
/// - The XXH3-64 is what keeps the document's digests one width and one spelling, and it
///   costs nothing: an attacker able to invert the pair could invert the sha256 alone.
///
/// **It also preserves comparability.** PostgreSQL's role digest was already this recipe
/// with the sha256 computed by the server, so a fingerprint taken before redaction existed
/// still compares against one taken after, provided the hex stays lowercase and the XXH3 is
/// taken over the hex characters rather than the 32 raw bytes. Both are true here, and the
/// tests hold them.
///
/// **What a digest does not do.** It proves change; it does not hide a *guessable* value. A
/// digest of a boolean has two possible answers and a digest of a weak password is
/// crackable, sha256 or not. Redaction keeps a secret out of the document, which is the
/// claim the design makes for it, and it is not a defence for a low-entropy value that
/// should not have been in a fingerprint at all.
pub fn redacted(scalar: &Scalar) -> Option<String> {
    let material = material_of(scalar)?;
    let sha256 = Sha256::digest(material.as_bytes());
    let hex = sha256
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();

    Some(format!(
        "{REDACTION_PREFIX}{}",
        Xxh3Digest::of(hex.as_bytes()).as_str()
    ))
}

/// The characters a scalar contributes to its digest, or nothing for a null.
///
/// A null withholds nothing, so digesting it would replace an honest absence with a
/// stand-in for a value that was never there.
fn material_of(scalar: &Scalar) -> Option<String> {
    match scalar {
        Scalar::Null => None,
        Scalar::Boolean(value) => Some(value.to_string()),
        Scalar::Integer(value) => Some(value.to_string()),
        Scalar::Text(value) => Some(value.clone()),
    }
}
