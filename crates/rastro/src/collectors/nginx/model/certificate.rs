//! A certificate a virtual host serves.

use rastro_collector::{NonEmptyText, Observation};

/// An `ssl_certificate` and the `ssl_certificate_key` beside it, as the configuration writes
/// them.
///
/// **Paired by position, the way nginx pairs them.** A host serving both an RSA and an
/// ECDSA certificate declares two of each directive, and the first key belongs to the first
/// certificate. Anything else would put the wrong key beside the wrong certificate in the
/// document.
///
/// Kept as written rather than resolved: a path may be relative to the prefix, may hold a
/// variable that is only known per request, or may be `data:` with the certificate inline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Certificate {
    pub certificate: NonEmptyText,
    pub key: Option<NonEmptyText>,
}

impl From<&Certificate> for Observation {
    fn from(certificate: &Certificate) -> Self {
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
        ])
    }
}
