//! Which manager reported a set of packages.

/// A package manager rastro can read.
///
/// The facet is keyed by this rather than merging every manager into one list, so a box
/// carrying two needs no arbitrary precedence and the two shapes may differ honestly:
/// dpkg reports a desired state and packages that are *not* installed, apk does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PackageManager {
    Apk,
    Dpkg,
}

impl PackageManager {
    /// Every manager rastro knows how to read.
    ///
    /// Here rather than in the source layer because this is the file the compiler sends you to
    /// first when a variant is added: the match below stops the build three lines away.
    ///
    /// **A variant added without extending this list is still built and never probed**, and
    /// nothing will say so. No test closes that, because a test would need its own copy of the
    /// list, and no shape closes it in stable Rust without a derive macro, which is not worth a
    /// dependency for two variants.
    pub const ALL: [Self; 2] = [Self::Apk, Self::Dpkg];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Apk => "apk",
            Self::Dpkg => "dpkg",
        }
    }
}
