//! The bytes an entry's metadata digest is taken over.

use rastro_collector::Xxh3Digest;

/// The bytes a digest is taken over, assembled so that no two different entries can produce
/// the same input.
///
/// **Every field is either fixed width or length-prefixed.** Concatenating text directly
/// would let two entries collide by construction rather than by luck: mode `0644` with owner
/// `0` and mode `064` with owner `40` are the same bytes run together. That is not a
/// hash-strength question and no hash would fix it.
///
/// Big-endian throughout, so the digest of a host does not depend on the endianness of the
/// machine that walked it.
#[derive(Debug, Default)]
pub struct CanonicalBytes(Vec<u8>);

impl CanonicalBytes {
    pub fn new() -> Self {
        Self::default()
    }

    /// A field that is present, tagged so its absence is a different input.
    pub fn text(mut self, value: &str) -> Self {
        self.0.push(1);
        self.0
            .extend_from_slice(&(value.len() as u64).to_be_bytes());
        self.0.extend_from_slice(value.as_bytes());

        self
    }

    pub fn integer(mut self, value: i64) -> Self {
        self.0.push(1);
        self.0.extend_from_slice(&value.to_be_bytes());

        self
    }

    /// A field this entry does not have, which is meaning rather than nothing: a directory has
    /// no size, a symlink has a target and a regular file does not.
    pub fn absent(mut self) -> Self {
        self.0.push(0);

        self
    }

    /// A field withheld because it moves on its own, which must not be conflated with a field
    /// the entry does not have — otherwise a churning file and a directory could agree.
    pub fn withheld(mut self) -> Self {
        self.0.push(2);

        self
    }

    /// One field, in whichever of the three states it is in.
    ///
    /// Withheld wins over absent: a volatile field's value is not read into the digest at all,
    /// so whether it happened to have one is not something the digest may depend on.
    pub fn maybe_integer(self, withheld: bool, value: Option<i64>) -> Self {
        match (withheld, value) {
            (true, _) => self.withheld(),
            (false, Some(value)) => self.integer(value),
            (false, None) => self.absent(),
        }
    }

    pub fn maybe_text(self, withheld: bool, value: Option<&str>) -> Self {
        match (withheld, value) {
            (true, _) => self.withheld(),
            (false, Some(value)) => self.text(value),
            (false, None) => self.absent(),
        }
    }

    /// The one value the default view records for an entry.
    ///
    /// **This answers the question that view exists to answer**: did anything about this path
    /// change between two runs. It replaces the eleven attributes the entry used to list,
    /// which cost 444 bytes a path and made a document of 31,000 entries 13 MB. Since the
    /// document names every path on the box, its floor is the path strings themselves, and a
    /// digest per path lands within a fifth of that floor. What is lost is *which* attribute
    /// moved, and [`Detail`](super::Detail) is how to ask.
    pub fn digest(&self) -> Xxh3Digest {
        Xxh3Digest::of(&self.0)
    }
}
