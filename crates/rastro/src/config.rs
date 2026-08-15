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
//!
//! A plain settings type. It knows nothing of the document model: shaping the
//! effective config into an observation belongs to the collector that reports
//! it.

use std::path::Path;

use serde::Deserialize;
use thiserror::Error;

/// The settings in effect for a run.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    collectors: Collectors,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct Collectors {
    #[serde(default)]
    exclude: Vec<String>,
}

/// The config file could not be read as one.
///
/// Both variants name the file. An operator with a broken `/etc/rastro.toml`
/// should not have to guess which of their configs the parser was looking at.
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("could not read {path}: {source}")]
    Unreadable {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("could not parse {path}: {source}")]
    Malformed {
        path: String,
        #[source]
        source: toml::de::Error,
    },
}

impl Config {
    /// Reads a config from TOML.
    ///
    /// Exclusions are sorted and deduplicated here, so that two configs meaning
    /// the same thing produce the same document. The effective config is in the
    /// envelope precisely so runs can be compared; it must not differ because
    /// someone listed the same collector twice.
    ///
    /// Unknown keys and tables are refused rather than ignored: a misspelled
    /// `excludes` that silently does nothing would leave the operator believing
    /// a collector was switched off when it was still running.
    pub fn parse(toml: &str) -> Result<Self, toml::de::Error> {
        let mut config: Self = toml::from_str(toml)?;
        config.collectors.exclude.sort_unstable();
        config.collectors.exclude.dedup();

        Ok(config)
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

        Self::parse(&toml).map_err(|source| ConfigError::Malformed {
            path: path.display().to_string(),
            source,
        })
    }

    /// The collectors the operator asked not to run, sorted and without
    /// duplicates.
    pub fn excluded(&self) -> &[String] {
        &self.collectors.exclude
    }
}
