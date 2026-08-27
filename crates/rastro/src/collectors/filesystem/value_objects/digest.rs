//! What a file's content reduces to.

use crate::collectors::filesystem::value_objects::DigestAlgorithm;

/// A digest of one file's content, carrying the algorithm that produced it.
///
/// The algorithm travels with the value rather than being assumed once in the envelope,
/// because the pair is what a reader needs: two fingerprints hashed differently cannot be
/// diffed against each other, and a bare hex string gives no way to tell that has happened.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Digest {
    algorithm: DigestAlgorithm,
    value: String,
}

impl Digest {
    /// Renders the raw digest bytes as lowercase hex, which is the form `sha256sum` prints
    /// and therefore the form an operator can reproduce by hand.
    pub fn of(algorithm: DigestAlgorithm, bytes: &[u8]) -> Self {
        let mut value = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            value.push_str(&format!("{byte:02x}"));
        }

        Self { algorithm, value }
    }

    pub fn algorithm(&self) -> DigestAlgorithm {
        self.algorithm
    }

    pub fn as_str(&self) -> &str {
        &self.value
    }
}
