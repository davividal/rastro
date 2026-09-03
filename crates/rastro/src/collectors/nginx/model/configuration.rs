//! A configuration, assembled from every file nginx would read.

use rastro_collector::{AbsolutePath, Observation};

use crate::collectors::nginx::model::{ConfigurationFile, Directive};

/// The whole configuration: what was read, and what it says.
///
/// **`directives` has its includes spent.** An `include` is gone by the time it reaches
/// here, replaced in place by the directives of the files it named, which is what nginx does
/// and what makes a `server` block in `conf.d/site.conf` a server of the `http` block that
/// included it. `files` is the record of that assembly, in the order nginx would have read
/// them.
///
/// The reading order is kept rather than sorted, for the reason the mount table keeps the
/// kernel's: order is state here. Two files that both define a `default_server` are resolved
/// by which was read first, so a sort would discard the fact that decides the outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Configuration {
    /// What the configuration's relative paths are relative to.
    pub prefix: AbsolutePath,
    /// The file nginx would open first.
    pub root: AbsolutePath,
    pub files: Vec<ConfigurationFile>,
    pub directives: Vec<Directive>,
}

impl From<&Configuration> for Observation {
    fn from(configuration: &Configuration) -> Self {
        Observation::object([
            (
                "files",
                Observation::list(configuration.files.iter().map(Observation::from)),
            ),
            ("prefix", Observation::text(configuration.prefix.as_str())),
            ("root", Observation::text(configuration.root.as_str())),
        ])
    }
}
