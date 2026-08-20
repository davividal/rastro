//! Which system a set of repositories was configured for.

/// A repository configuration system rastro can read.
///
/// **Not the same list as [`PackageManager`], and the difference is the reason this
/// enum exists rather than the other one being reused.** On Debian, `dpkg` owns the
/// installed-package database and `apt` owns the repository list: they are two tools,
/// two file trees, and a box can have the first without the second. On Alpine, `apk`
/// is both. So the axis the packages facet is keyed by and the axis this one is keyed
/// by genuinely differ, and sharing one enum would force a `Dpkg` variant that has no
/// repositories and an `Apt` variant that installs nothing.
///
/// [`PackageManager`]: crate::collectors::packages::PackageManager
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RepositorySystem {
    Apk,
    Apt,
}

impl RepositorySystem {
    /// Every system rastro knows how to read.
    ///
    /// Here rather than in the source layer for the reason the packages collector
    /// gives for the same list: this is the file the compiler sends you to first when
    /// a variant is added, because the match below stops the build three lines away.
    ///
    /// **A variant added without extending this list is still built and never
    /// probed**, and nothing will say so. The same hole the packages collector
    /// documents, for the same reason: closing it needs a derive macro, and that is
    /// not worth a dependency for two variants.
    ///
    /// The obvious next three are `Dnf` (`/etc/yum.repos.d`), `Pacman`
    /// (`/etc/pacman.conf` and its `Include` directives) and `Zypper`
    /// (`/etc/zypp/repos.d`). Each is a new source and a new variant, and neither the
    /// model nor this facet's shape has to change to take them.
    pub const ALL: [Self; 2] = [Self::Apk, Self::Apt];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Apk => "apk",
            Self::Apt => "apt",
        }
    }
}
