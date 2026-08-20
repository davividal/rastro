//! The key material itself.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// A public key, base64 as the file spells it.
///
/// # Why this is recorded in full, where a password hash is not
///
/// The accounts facet deliberately records no password hash, because a hash is a credential
/// verifier: possessing one lets an attacker crack the password offline, so putting one in a
/// document is a real leak while rastro has no redaction layer.
///
/// **A public key is the opposite, and the difference is not a matter of degree.** It is
/// public by construction — it is handed to every server its owner logs into and to anybody
/// who asks — and possessing it grants nothing. So recording it costs nothing and buys the
/// thing this facet exists for: **the key body is the access grant**, and a key swapped for a
/// different one under the same comment and the same algorithm is invisible in every other
/// field. That is precisely the change an audit needs to see.
///
/// It is therefore *not* annotated sensitive. Marking it so would be cargo-culting the
/// accounts decision onto a value where the reasoning does not apply, and it would hide the
/// most useful thing here the day redaction lands.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PublicKey(NonEmptyText);

impl PublicKey {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "public key")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&PublicKey> for Observation {
    fn from(key: &PublicKey) -> Self {
        Observation::text(key.as_str())
    }
}
