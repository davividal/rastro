//! apt's repository configuration, across both of its formats.

use std::fs;
use std::path::{Path, PathBuf};

use rastro_collector::CollectionError;

use super::{apt_deb822, apt_one_line};
use crate::collectors::repositories::model::{Repository, RepositorySet};

/// apt's configuration root.
const ETC_APT: &str = "/etc/apt";

/// The single one-line file apt reads directly under its root.
const SOURCES_LIST: &str = "sources.list";

/// The drop-in directory, where both formats live side by side.
const SOURCES_LIST_D: &str = "sources.list.d";

/// The extension that marks a one-line file, and the one that marks a deb822 file.
///
/// **apt reads only these two and silently ignores everything else in the directory**,
/// which is why rastro must ignore them too. A `postgres.list.save` left behind by
/// `dpkg` is not a repository, and reporting it as one would put a repository in the
/// fingerprint that the host does not use. The corollary is worth knowing: disabling a
/// repository by renaming its file to `.list.disabled` makes it vanish from this facet
/// entirely rather than appear as disabled, because it has vanished from apt too.
const ONE_LINE_EXTENSION: &str = "list";
const DEB822_EXTENSION: &str = "sources";

/// apt's repository configuration as a source rastro can read.
///
/// # Why the files and not `apt-get indextargets`
///
/// The design rule is to prefer effective, resolved state over configuration files,
/// and this collector reads configuration files. That is a considered exception rather
/// than an oversight, so here is the reasoning.
///
/// `apt-get indextargets` is the closest thing apt has to `nginx -T`. It resolves both
/// formats into one list and adds each repository's `Origin`, `Label`, `Suite` and
/// `Version` from the downloaded release metadata. Three things rule it out. It
/// enumerates *index files apt would fetch*, one paragraph per suite, component, type
/// and language, so a box with three suites produces dozens of paragraphs keyed by
/// paths into `/var/lib/apt/lists`. Those paths, and the metadata fields, come from the
/// last `apt update`, so the answer changes when nothing about the configuration has,
/// and a repository added but never updated is reported differently from the same
/// repository after an update. And it omits disabled entries altogether, which are the
/// most useful thing in the file: swapping a repository is done by commenting one line
/// and adding another.
///
/// It also does not resolve the one indirection that would have justified the cost:
/// `mirror+file:` URIs come back unresolved either way.
///
/// The rule's purpose is to catch a meaning that changed without a file changing. For
/// apt there is no such gap. `apt.conf.d` tunes how repositories are fetched and cannot
/// add or remove one, so the configuration *is* the state, and reading it is exact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AptSources {
    root: PathBuf,
}

impl AptSources {
    pub fn new() -> Self {
        Self {
            root: PathBuf::from(ETC_APT),
        }
    }

    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Finds apt's configuration, or reports that this host does not use apt.
    ///
    /// The configuration root rather than the `apt-get` binary, which is the opposite
    /// of how the packages collector detects dpkg, and for a reason: what is being
    /// asked here is where the host is configured to fetch packages from, and that
    /// question is answered by the files. A box could carry the binary with no
    /// configuration, and a container image can carry the configuration with the
    /// binary removed; in both cases the files are the honest answer.
    pub fn detect() -> Option<Self> {
        let sources = Self::new();
        sources.root.is_dir().then_some(sources)
    }

    /// Reads every file apt would read, in both formats.
    pub fn read(&self) -> Result<RepositorySet, CollectionError> {
        let mut repositories = Vec::new();

        let sources_list = self.root.join(SOURCES_LIST);
        if sources_list.is_file() {
            repositories.extend(self.read_one_line(&sources_list)?);
        }

        for path in self.drop_ins()? {
            match extension_of(&path).as_deref() {
                Some(ONE_LINE_EXTENSION) => repositories.extend(self.read_one_line(&path)?),
                Some(DEB822_EXTENSION) => repositories.extend(self.read_deb822(&path)?),
                _ => {}
            }
        }

        Ok(RepositorySet::new(repositories))
    }

    /// The drop-in files, sorted so that a failure names the same file on two runs.
    fn drop_ins(&self) -> Result<Vec<PathBuf>, CollectionError> {
        let directory = self.root.join(SOURCES_LIST_D);
        if !directory.is_dir() {
            return Ok(Vec::new());
        }

        let mut paths = Vec::new();
        for entry in fs::read_dir(&directory).map_err(|error| {
            CollectionError::new(format!("could not list {}: {error}", directory.display()))
        })? {
            let entry = entry.map_err(|error| {
                CollectionError::new(format!(
                    "could not list an entry of {}: {error}",
                    directory.display()
                ))
            })?;
            paths.push(entry.path());
        }
        paths.sort();

        Ok(paths)
    }

    fn read_one_line(&self, path: &Path) -> Result<Vec<Repository>, CollectionError> {
        let text = self.contents_of(path)?;
        let mut repositories = Vec::new();

        for line in text.lines() {
            if let Some(repository) = apt_one_line::parse_line(line)
                .map_err(|error| CollectionError::new(format!("in {}: {error}", path.display())))?
            {
                repositories.push(repository);
            }
        }

        Ok(repositories)
    }

    fn read_deb822(&self, path: &Path) -> Result<Vec<Repository>, CollectionError> {
        let text = self.contents_of(path)?;

        apt_deb822::parse(&text)
            .map_err(|error| CollectionError::new(format!("in {}: {error}", path.display())))
    }

    fn contents_of(&self, path: &Path) -> Result<String, CollectionError> {
        fs::read_to_string(path).map_err(|error| {
            CollectionError::new(format!("could not read {}: {error}", path.display()))
        })
    }
}

fn extension_of(path: &Path) -> Option<String> {
    path.extension()
        .map(|extension| extension.to_string_lossy().into_owned())
}

impl Default for AptSources {
    fn default() -> Self {
        Self::new()
    }
}
