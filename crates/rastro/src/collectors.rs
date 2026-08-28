//! The collectors that ship inside the binary.

pub mod accounts;
pub mod block_devices;
pub mod canonical_tool;
pub mod cron;
pub mod exporters;
pub mod filesystem;
pub mod firewall;
pub mod inet;
// Private: the flat re-exports below are the whole outside surface.
mod host;
mod invocation;
pub mod locale;
pub mod modules;
pub mod mounts;
pub mod network;
pub mod packages;
pub mod postgresql;
pub mod processes;
pub mod repositories;
pub mod sockets;
pub mod ssh_access;
pub mod sysctl;
pub mod systemd;
pub mod time;
pub mod timers;
pub mod units;

pub use accounts::AccountsCollector;
pub use block_devices::BlockDevicesCollector;
pub use cron::CronCollector;
pub use exporters::ExportersCollector;
pub use filesystem::FilesystemCollector;
pub use firewall::FirewallCollector;
pub use host::HostCollector;
pub use invocation::{InvocationCollector, effective_config, seconds_since_epoch};
pub use locale::LocaleCollector;
pub use modules::ModulesCollector;
pub use mounts::MountsCollector;
pub use network::NetworkCollector;
pub use packages::PackagesCollector;
pub use postgresql::PostgresqlCollector;
pub use processes::ProcessesCollector;
pub use repositories::RepositoriesCollector;
pub use sockets::SocketsCollector;
pub use ssh_access::SshAccessCollector;
pub use sysctl::SysctlCollector;
pub use time::TimeCollector;
pub use timers::TimersCollector;
pub use units::UnitsCollector;

use rastro_collector::{CollectionError, Collector, CollectorCategory, Observation};

use thiserror::Error;

use crate::collectors::filesystem::WalkPolicy;

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
///
/// Two collectors are appended rather than listed, because each needs something the others
/// produce: the filesystem walk runs under the table the others' claims resolve to, and the
/// `invocation` facet reports that table. Gathering the claims here is what keeps the two
/// collectors ignorant of each other.
pub fn built_in(effective_config: Observation) -> Vec<Box<dyn Collector>> {
    let mut collectors = state_collectors();
    let policy = claimed_policy(&collectors);
    let table = match &policy {
        Ok(resolved) => Observation::from(resolved),
        Err(_) => Observation::null(),
    };

    collectors.push(Box::new(FilesystemCollector::under(policy)));
    collectors.push(Box::new(InvocationCollector::new(effective_config, table)));
    collectors
}

/// The collectors that observe the host, filesystem aside.
fn state_collectors() -> Vec<Box<dyn Collector>> {
    vec![
        Box::new(AccountsCollector::new()),
        Box::new(BlockDevicesCollector::new()),
        Box::new(CronCollector::new()),
        Box::new(ExportersCollector::new()),
        Box::new(FirewallCollector::new()),
        Box::new(HostCollector::new()),
        Box::new(LocaleCollector::new()),
        Box::new(ModulesCollector::new()),
        Box::new(MountsCollector::new()),
        Box::new(NetworkCollector::new()),
        Box::new(PackagesCollector::new()),
        Box::new(PostgresqlCollector::new()),
        Box::new(ProcessesCollector::new()),
        Box::new(RepositoriesCollector::new()),
        Box::new(SocketsCollector::new()),
        Box::new(SshAccessCollector::new()),
        Box::new(SysctlCollector::new()),
        Box::new(TimeCollector::new()),
        Box::new(TimersCollector::new()),
        Box::new(UnitsCollector::new()),
    ]
}

/// The shipped table with every collector's claims folded in, or the conflict that stopped
/// it resolving.
///
/// A claim is asked of every built-in collector, including one the config will exclude in a
/// moment. That is deliberate and it is the narrower of the two wrong answers: releasing a
/// claim because its facet was excluded would make an exclusion *widen* the walk, and
/// `--exclude postgresql` would silently put a cluster's data directory back under the
/// hashing default.
fn claimed_policy(collectors: &[Box<dyn Collector>]) -> Result<WalkPolicy, CollectionError> {
    collectors
        .iter()
        .map(AsRef::as_ref)
        .try_fold(WalkPolicy::built_in(), |policy, collector| {
            policy.claimed(collector.name(), &collector.filesystem_claims())
        })
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
    all: Vec<Box<dyn Collector>>,
    config: &Config,
) -> Result<Selection, SelectionError> {
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
