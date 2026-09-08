//! A content address an engine resolved.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// An `algorithm:hex` digest, as the registry world spells it: `sha256:28bd5f…`.
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

        let Some((algorithm, hex)) = spelled.split_once(':') else {
            return Err(CollectionError::new(format!(
                "the engine reported the digest {spelled:?}, which names no algorithm"
            )));
        };

        if algorithm.is_empty()
            || !algorithm
                .chars()
                .all(|character| character.is_ascii_alphanumeric())
        {
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
