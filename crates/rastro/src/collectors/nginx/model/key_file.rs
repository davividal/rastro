//! The private key beside a certificate, described and never opened.

use rastro_collector::{AbsolutePath, NonEmptyText, Observation};

use crate::collectors::file_metadata::FileMode;

/// Where a private key lives, and who can read it.
///
/// **Never its content, and never a digest of its content.** A digest would be a way to
/// confirm a guessed key, and there is nothing a fingerprint could do with it that is worth
/// that. What the facet records is the thing an operator actually wants to catch: a key that
/// became group- or world-readable, or changed owner, between two runs.
///
/// The `filesystem` facet walks this path too and carries the same bytes under its own lens.
/// It is here as well because a key among half a million entries is not something anybody
/// finds, and beside the certificate it protects it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyFile {
    pub path: AbsolutePath,
    pub reading: KeyReading,
}

/// What was learned about the key file, or why nothing was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyReading {
    Described {
        mode: FileMode,
        owner: i64,
        group: i64,
    },
    Refused {
        reason: NonEmptyText,
    },
}

impl From<&KeyFile> for Observation {
    fn from(key: &KeyFile) -> Self {
        let reading = match &key.reading {
            KeyReading::Described { mode, owner, group } => Observation::object([
                ("group", Observation::integer(*group)),
                ("mode", Observation::text(mode.as_str())),
                ("owner", Observation::integer(*owner)),
            ]),
            KeyReading::Refused { reason } => Observation::text(reason.as_str()),
        };

        Observation::object([
            ("path", Observation::text(key.path.as_str())),
            ("reading", reading),
        ])
    }
}
