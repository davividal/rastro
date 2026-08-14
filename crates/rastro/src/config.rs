//! What the operator asked rastro *not* to do.
//!
//! Optional, opt-in and explicit. With no config every collector runs, because
//! the premise of the tool is a box nobody documented: requiring a declaration
//! of what to look at is the same disqualifier that ruled out AIDE and
//! configsnap, one level up. See `docs/decisions.md`.
//!
//! There is deliberately no way to say which collectors *do* run. Exclusions
//! can only narrow, so a config can never hide a state surface the operator did
//! not know to ask for.

use std::path::{Path, PathBuf};

use rastro_collector::Observation;
use serde::Deserialize;
use thiserror::Error;

/// The settings in effect for a run.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    collectors: Collectors,

    /// Where this came from, carried so the document can say. `None` is the
    /// default config, which is what runs when no `--config` was given.
    #[serde(skip)]
    source: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct Collectors {
    #[serde(default)]
    exclude: Vec<String>,
}

/// The config could not be read as one.
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("could not read {path}: {source}")]
    Unreadable {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("{0}")]
    Malformed(#[from] toml::de::Error),

    #[error(
        "no collector is named {name:?}, so excluding it would do nothing; available: {available}"
    )]
    UnknownCollector { name: String, available: String },

    #[error(
        "{name:?} is a metadata collector and cannot be excluded: without it one \
         fingerprint cannot be told apart from another"
    )]
    MetadataCollector { name: String },
}

impl Config {
    /// Reads a config from TOML.
    ///
    /// Unknown keys and tables are refused rather than ignored: a misspelled
    /// `excludes` that silently does nothing would leave the operator believing
    /// a collector was switched off when it was still running.
    pub fn parse(toml: &str) -> Result<Self, ConfigError> {
        Ok(toml::from_str(toml)?)
    }

    /// Reads a config from a file.
    ///
    /// A path that was given explicitly and cannot be read is an error, never a
    /// silent fall back to the defaults: the run would then be wider than the
    /// operator asked for, and the diff would not say so.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let toml = std::fs::read_to_string(path).map_err(|source| ConfigError::Unreadable {
            path: path.display().to_string(),
            source,
        })?;

        let mut config = Self::parse(&toml)?;
        config.source = Some(path.to_path_buf());

        Ok(config)
    }

    /// The collectors the operator asked not to run.
    pub fn excluded(&self) -> &[String] {
        &self.collectors.exclude
    }

    pub fn source(&self) -> Option<&Path> {
        self.source.as_deref()
    }

    /// How this config appears in the `invocation` facet.
    ///
    /// Recorded whether it came from a file or from the defaults, so two runs
    /// under different scope cannot be diffed without the difference showing.
    pub fn as_observation(&self) -> Observation {
        Observation::object([
            (
                "excluded_collectors",
                Observation::list(
                    self.excluded()
                        .iter()
                        .map(|name| Observation::text(name.as_str())),
                ),
            ),
            (
                "source",
                match self.source() {
                    Some(path) => Observation::text(path.display().to_string()),
                    None => Observation::null(),
                },
            ),
        ])
    }
}
