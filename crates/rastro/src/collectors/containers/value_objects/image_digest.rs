//! A content address an engine resolved.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// A content address, in either spelling an engine uses: `sha256:28bd5f…` or bare hex.
///
/// **Two engines, two spellings of one value, and neither is normalised into the other.**
/// docker writes an image's id as `sha256:` and the hex; podman's list prints the hex alone.
/// Rewriting podman's into docker's would mean rastro asserting an algorithm the engine
/// never named, and the rule here is the one the versions follow: record what the host
/// reported.
///
/// **The value this whole facet exists for.** A tag says what was asked for and moves under
/// the operator's feet; a digest says what is running. `nginx:1.29` rebuilt upstream and
/// pulled again is the change a file-hashing tool cannot see and a diff of these two lines
/// makes obvious.
///
/// The shape is checked rather than trusted: an engine that answered with a truncated or
/// empty digest has been misread, and a misread content address is worse than a recorded
/// failure because it reads as a real one.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ImageDigest(NonEmptyText);

impl ImageDigest {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        let text = NonEmptyText::new(value, "image digest")?;
        let spelled = text.as_str();

        let (algorithm, hex) = match spelled.split_once(':') {
            Some((algorithm, hex)) => (Some(algorithm), hex),
            None => (None, spelled),
        };

        let named = algorithm.is_none_or(|algorithm| {
            !algorithm.is_empty()
                && algorithm
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric())
        });
        if !named {
            return Err(CollectionError::new(format!(
                "the engine reported the digest {spelled:?}, whose algorithm is not a word"
            )));
        }

        if hex.is_empty() || !hex.chars().all(|character| character.is_ascii_hexdigit()) {
            return Err(CollectionError::new(format!(
                "the engine reported the digest {spelled:?}, whose value is not hexadecimal"
            )));
        }

        Ok(Self(text))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&ImageDigest> for Observation {
    fn from(digest: &ImageDigest) -> Self {
        Observation::text(digest.as_str())
    }
}
