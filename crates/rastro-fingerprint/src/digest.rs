//! The one way a value is reduced to something a document may carry.

use xxhash_rust::xxh3::xxh3_64;

/// A digest of bytes, spelled the way every facet spells one.
///
/// **What it is for.** Answering *did this change* about a value the document should not or
/// need not carry whole: too large to repeat per entry, or material a fingerprint has no
/// business holding. A digest is only ever compared with the digest of the *same subject* in
/// another run, and every property below follows from that.
///
/// **The algorithm is the contract.** XXH3-64, rendered as sixteen lowercase hex characters.
/// Changing either invalidates every archived fingerprint, which is why it is a named,
/// specified algorithm and not `DefaultHasher`, whose output std explicitly declines to keep
/// stable across releases.
///
/// **Why it lives in this crate.** Two collectors inventing their own would put two digest
/// spellings in one document, which is why it was in the collector port to begin with. It
/// moved down here when redaction landed, because the *renderer* spells one too: a
/// sensitive value stands in as a digest, and the substitution is a rule about observations
/// rather than about collecting. The port re-exports it, so a collector's import is
/// unchanged.
///
/// **Sixty-four bits, and that is not a compromise.** A collision between two different
/// subjects means nothing, because their digests are never compared. The only failure is a
/// subject that changed and hashed the same anyway, at 2⁻⁶⁴ per changed subject; across the
/// 46,000 entries of a whole-host walk the birthday bound is still ~6e-11.  Width is also
/// what drives document size, at four bytes of document per byte of digest.
///
/// **Not cryptographic, and for a walked file it does not need to be.** Nothing here defends
/// against a forged digest: an attacker who can change the subject can change what it hashes
/// to. What the digest defends against is the *document* carrying the material, which it
/// cannot, because a digest is one way and this one keeps no salt.
///
/// That last argument is weaker for a **secret** than for a file's attributes, which is why
/// redaction does not hand a secret straight to this type. See
/// [`observation::redaction`](crate::observation::redaction).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Xxh3Digest(u64);

impl Xxh3Digest {
    pub fn of(material: &[u8]) -> Self {
        Self(xxh3_64(material))
    }

    /// Sixteen lowercase hex characters, zero-padded.
    ///
    /// Fixed width because a digest that varied in length would sort and align
    /// inconsistently between entries, and this is the value a reader's eye runs down a diff
    /// comparing.
    pub fn as_str(&self) -> String {
        format!("{:016x}", self.0)
    }
}
