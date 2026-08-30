//! How much of an entry the document carries.

/// Whether an entry is recorded as a digest of its metadata or as the metadata itself.
///
/// **A third axis, beside the view and the format.** A view says *what is in* the document and
/// a format says what it looks like; this says how finely one path is described. It is a
/// separate question from volatility: `Summary` still withholds a directory's derived stamps,
/// because the digest is taken over exactly the attributes the view would have kept.
///
/// **Chosen before the run, and that is the cost.** A fingerprint taken as a summary cannot be
/// expanded afterwards, so an operator who wants to know *which* attribute moved has to have
/// asked at the time. `Summary` is still the default, because the question a fingerprint is
/// taken to answer is "what changed", and the answer to "how" is a second question that is
/// usually asked about a handful of paths rather than all of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Detail {
    /// One digest per path.
    Summary,
    /// Every attribute, as the walk read it.
    Full,
}

impl Detail {
    /// How the effective config spells it, since a document taken one way cannot be compared
    /// with one taken the other.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Summary => "summary",
            Self::Full => "full",
        }
    }
}
