//! Whether a module can ever be taken out again.

use rastro_collector::Observation;

/// Whether a module can be unloaded.
///
/// A module with an init function and no exit function can never be removed, and the kernel
/// says so by listing `[permanent]` among its dependants. Recorded as its own value rather
/// than left in the dependant list, where it would read as a module called `[permanent]`.
///
/// Named rather than a `bool` so the document says `"permanent"` instead of
/// `"removability": true`, which would leave the reader guessing which way round it runs.
///
/// Three-valued for the same reason [`ReferenceCount`](super::reference_count::ReferenceCount)
/// is: a kernel built without `CONFIG_MODULE_UNLOAD` compiles out the code that prints
/// `[permanent]` at all, so on such a kernel *nothing* can be unloaded and the absence of the
/// marker means the opposite of what it means elsewhere. Answering `Removable` there would be
/// a confident lie about every module on the box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Removability {
    Removable,
    Permanent,

    /// This kernel does not track unloading, so the question has no answer to read.
    Unknown,
}

impl Removability {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Removable => "removable",
            Self::Permanent => "permanent",
            Self::Unknown => "unknown",
        }
    }
}

impl From<&Removability> for Observation {
    fn from(removability: &Removability) -> Self {
        Observation::text(removability.as_str())
    }
}
