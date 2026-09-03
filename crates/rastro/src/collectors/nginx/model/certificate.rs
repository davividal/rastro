//! A certificate a virtual host serves.

use rastro_collector::{NonEmptyText, Observation};

use crate::collectors::nginx::model::{CertificateDetails, KeyFile};

/// An `ssl_certificate` and the `ssl_certificate_key` beside it, as the configuration writes
/// them and as the files themselves read.
///
/// **Paired by position, the way nginx pairs them.** A host serving both an RSA and an
/// ECDSA certificate declares two of each directive, and the first key belongs to the first
/// certificate. Anything else would put the wrong key beside the wrong certificate in the
/// document.
///
/// The written paths are kept as written *and* the file is read, because the two answer
/// different questions: the path is what an operator edits, and the certificate is what
/// clients are handed. A path may also be no path at all — a variable resolved per request,
/// or a `data:` certificate written inline — and then the reading says so.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Certificate {
    pub certificate: NonEmptyText,
    pub key: Option<NonEmptyText>,
    pub reading: CertificateReading,
    pub key_file: Option<KeyFile>,
}

/// The certificate as the file reads, or why it does not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CertificateReading {
    Parsed(Box<CertificateDetails>),
    Refused { reason: NonEmptyText },
}

impl From<&Certificate> for Observation {
    fn from(certificate: &Certificate) -> Self {
        let reading = match &certificate.reading {
            CertificateReading::Parsed(details) => Observation::from(details.as_ref()),
            CertificateReading::Refused { reason } => Observation::text(reason.as_str()),
        };

        Observation::object([
            (
                "certificate",
                Observation::text(certificate.certificate.as_str()),
            ),
            (
                "key",
                certificate
                    .key
                    .as_ref()
                    .map_or_else(Observation::null, |key| Observation::text(key.as_str())),
            ),
            (
                "key_file",
                certificate
                    .key_file
                    .as_ref()
                    .map_or_else(Observation::null, Observation::from),
            ),
            ("reading", reading),
        ])
    }
}
