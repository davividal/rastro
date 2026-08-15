//! The collectors that ship inside the binary.

mod host;
mod invocation;
mod mounts;

pub use host::HostCollector;
pub use invocation::{InvocationCollector, effective_config, seconds_since_epoch};
pub use mounts::{MountsCollector, parse_mount_table};

use rastro_collector::{Collector, CollectorCategory};

use thiserror::Error;

use rastro_collector::Observation;

use crate::config::Config;

/// The config named a collector that cannot be excluded.
///
/// Separate from `ConfigError`, which is about reading the file: whether a
/// collector exists is a fact about this registry, not about TOML.
#[derive(Debug, Error)]
pub enum SelectionError {
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

/// Every built-in collector, in the order they are registered.
///
/// Order is irrelevant to the document, which sorts facets by name. The
/// effective config is a parameter only because one collector reports it; no
/// other registration reads it.
pub fn built_in(effective_config: Observation) -> Vec<Box<dyn Collector>> {
    vec![
        Box::new(HostCollector::new()),
        Box::new(InvocationCollector::new(effective_config)),
        Box::new(MountsCollector::new()),
    ]
}

/// The collectors a config leaves running, and the ones it took away.
///
/// Excluded collectors are omitted from the document entirely rather than
/// recorded `absent`: absence is something observed about the host, exclusion is
/// something the operator chose, and conflating them would put a decision into
/// the state.
pub struct Selection {
    running: Vec<Box<dyn Collector>>,
    excluded: Vec<String>,
}

/// Written by hand rather than derived, and rather than putting `Debug` on the
/// `Collector` trait: a collector author should not have to derive anything to
/// satisfy a test assertion in this crate. Names are what a failure needs.
impl std::fmt::Debug for Selection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Selection")
            .field("running", &names_of(&self.running))
            .field("excluded", &self.excluded)
            .finish()
    }
}

impl Selection {
    /// The collectors that survived the config, ready for the use case.
    pub fn running(&self) -> &[Box<dyn Collector>] {
        &self.running
    }

    /// Names the operator asked to leave out, so they can be warned about.
    pub fn excluded(&self) -> &[String] {
        &self.excluded
    }
}

/// Narrows the built-in collectors by a config.
///
/// Only ever narrows. There is no way to say which collectors run, so a config
/// can never hide a state surface the operator did not know to ask for.
pub fn selected(
    config: &Config,
    effective_config: Observation,
) -> Result<Selection, SelectionError> {
    let all = built_in(effective_config);

    for name in config.excluded() {
        let collector = all
            .iter()
            .find(|collector| collector.name().as_str() == name)
            .ok_or_else(|| SelectionError::UnknownCollector {
                name: name.clone(),
                available: names_of(&all).join(", "),
            })?;

        if collector.category() == CollectorCategory::Metadata {
            return Err(SelectionError::MetadataCollector { name: name.clone() });
        }
    }

    let running: Vec<Box<dyn Collector>> = all
        .into_iter()
        .filter(|collector| {
            !config
                .excluded()
                .iter()
                .any(|name| name == collector.name().as_str())
        })
        .collect();

    Ok(Selection {
        running,
        excluded: config.excluded().to_vec(),
    })
}

fn names_of(collectors: &[Box<dyn Collector>]) -> Vec<&str> {
    collectors
        .iter()
        .map(|collector| collector.name().as_str())
        .collect()
}
