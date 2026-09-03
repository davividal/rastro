//! A configuration, assembled from every file nginx would read.

use rastro_collector::{AbsolutePath, Observation};

use crate::collectors::nginx::model::{ConfigurationFile, Directive};
use crate::collectors::nginx::value_objects::{ConfigurationSource, SecondsSinceEpoch};

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
    /// What a cache or a temp path is relative to: `-p`, or `--prefix` at build time.
    pub prefix: AbsolutePath,
    /// What an `include`, a certificate and a user file are relative to: the directory of
    /// the root configuration file. nginx calls it `conf_prefix`, and on Debian it is a
    /// different directory from the prefix.
    pub configuration_prefix: AbsolutePath,
    /// The file nginx would open first.
    pub root: AbsolutePath,
    pub files: Vec<ConfigurationFile>,
    pub directives: Vec<Directive>,
    /// Which authority decided the root path: the running master, or the binary's own build.
    pub chosen_by: ConfigurationSource,
    /// The newest mtime among the files that were read.
    ///
    /// Held against the oldest worker's start time in the `master` node, this is the answer
    /// to the question the facet exists for: whether what is on disk is what is being
    /// served. It is an mtime and therefore only ever a *lower* bound on staleness — a file
    /// rewritten with the same content still moves it — which is why it sits beside the
    /// per-file digests rather than instead of them.
    pub newest_modified: Option<SecondsSinceEpoch>,
}

impl From<&Configuration> for Observation {
    fn from(configuration: &Configuration) -> Self {
        Observation::object([
            (
                "files",
                Observation::list(configuration.files.iter().map(Observation::from)),
            ),
            ("chosen_by", Observation::from(&configuration.chosen_by)),
            (
                "newest_modified",
                configuration
                    .newest_modified
                    .as_ref()
                    .map_or_else(Observation::null, Observation::from),
            ),
            (
                "configuration_prefix",
                Observation::text(configuration.configuration_prefix.as_str()),
            ),
            ("prefix", Observation::text(configuration.prefix.as_str())),
            ("root", Observation::text(configuration.root.as_str())),
        ])
    }
}
