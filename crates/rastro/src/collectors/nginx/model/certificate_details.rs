//! What a certificate on disk actually says.

use rastro_collector::{NonEmptyText, Observation, Xxh3Digest};

use crate::collectors::nginx::value_objects::SecondsSinceEpoch;

/// The certificate itself, read from the file the configuration points at.
///
/// **This is the difference between a renewal and an edit.** A path and an mtime say a file
/// changed; a serial, a validity window and a digest say whether the same authority reissued
/// the same names, or somebody put a different certificate there. Only one of those is
/// routine.
///
/// Nothing here is a secret. A certificate is what a server hands every client that
/// connects, and the private key beside it is never opened — see
/// [`KeyFile`](super::KeyFile), which records how it is protected and never what it holds.
///
/// **No "days remaining".** It would change between two runs of an unchanged host, which is
/// the one thing a value in this document may not do. The expiry is here as an instant, and
/// how close it is is the reader's arithmetic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertificateDetails {
    pub subject: NonEmptyText,
    pub issuer: NonEmptyText,
    pub serial: NonEmptyText,
    pub not_before: SecondsSinceEpoch,
    pub not_after: SecondsSinceEpoch,
    /// Every name the certificate is valid for, sorted.
    ///
    /// Sorted because the order in the extension is the issuer's, not the operator's, and
    /// two reissues of the same names must not read as a change.
    pub subject_alternative_names: Vec<NonEmptyText>,
    pub key_algorithm: NonEmptyText,
    /// A digest of the certificate's own bytes, so an identical reissue is still visible.
    pub digest: Xxh3Digest,
}

impl From<&CertificateDetails> for Observation {
    fn from(details: &CertificateDetails) -> Self {
        Observation::object([
            ("digest", Observation::from(&details.digest)),
            ("issuer", Observation::text(details.issuer.as_str())),
            (
                "key_algorithm",
                Observation::text(details.key_algorithm.as_str()),
            ),
            ("not_after", Observation::from(&details.not_after)),
            ("not_before", Observation::from(&details.not_before)),
            ("serial", Observation::text(details.serial.as_str())),
            ("subject", Observation::text(details.subject.as_str())),
            (
                "subject_alternative_names",
                Observation::list(
                    details
                        .subject_alternative_names
                        .iter()
                        .map(|name| Observation::text(name.as_str())),
                ),
            ),
        ])
    }
}
