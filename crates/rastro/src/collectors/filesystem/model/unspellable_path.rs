//! A path the walk reached and could not name.

use rastro_collector::{NonEmptyText, Observation};

/// An entry whose name is not valid UTF-8, reported as bytes because it has no name.
///
/// **Linux paths are bytes and the document holds text**, so a name like `b"\xff"` is legal on
/// disk and unsayable in the fingerprint. Substituting `U+FFFD` is the refusal `canonical_tool`
/// already makes for a tool's output, and for the same reason: it would put a path into the
/// document that is not on the box, and one nobody could act on.
///
/// **So it is reported without being named.** It cannot be a key, because a key here is a path
/// and this has none — which is why it lives in a list of its own rather than beside the
/// described paths. The bytes are given as lowercase hex, which claims nothing and is exact,
/// and the directory holding it is given as the text it almost always is, which is what makes
/// the report actionable: an operator can go and look.
///
/// The alternative was what this replaced: one such name anywhere on the box refused the entire
/// `filesystem` facet. That is a whole state surface lost to one extracted archive, and the
/// document did not say which file it was.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct UnspellablePath {
    /// Lowercase hex of the entry's own name, not of the whole path.
    pub name_bytes: String,
    /// The directory holding it.
    ///
    /// `Option` for the type's sake rather than for a case that arises: the walk never
    /// descends into a directory it could not name, so anything reported here was found in a
    /// directory that decoded. A `None` would mean that invariant had broken.
    pub directory: Option<String>,
}

impl UnspellablePath {
    pub fn of(bytes: &[u8], directory: Option<String>) -> Self {
        Self {
            name_bytes: bytes.iter().map(|byte| format!("{byte:02x}")).collect(),
            directory,
        }
    }
}

impl From<&UnspellablePath> for Observation {
    fn from(unspellable: &UnspellablePath) -> Self {
        Observation::object([
            (
                "directory",
                match &unspellable.directory {
                    Some(directory) => Observation::text(directory.as_str()),
                    None => Observation::null(),
                },
            ),
            (
                "error",
                Observation::text(
                    NonEmptyText::new(
                        "the name of this entry is not valid UTF-8, so it cannot be recorded \
                         as the path it is",
                        "a refusal",
                    )
                    .expect("a constant reason is not empty")
                    .as_str(),
                ),
            ),
            (
                "name_bytes",
                Observation::text(unspellable.name_bytes.as_str()),
            ),
        ])
    }
}
