//! Layer 3: what a redis or valkey server is actually running with.
//!
//! **The running server is the only honest account of itself.** On the estate that motivated
//! this facet, `maxmemory` and `save` are applied with `CONFIG SET` and never written to a file,
//! so the configuration on disk and the running server disagree by design and no walk of the
//! filesystem can see it. The facet asks the server.
pub mod model;
pub mod source;
pub mod value_objects;

pub use model::{
    Accounts, Installation, Instance, Module, Modules, Replication, ServerIdentity, Settings,
};
pub use source::{
    AclList, ConfigGet, Credential, DialTarget, DiscoveredServer, InfoReplication, InfoServer,
    InstalledServers, ModuleList, PasswordDirectives, Reply, ResidentServer, RespConnection,
    ServerCgroup, ServerStart, ServerStream, data_directories_of, default_user_in_acl_file,
    discover, password_directives_in, password_for, password_for_default_account,
    read_installation, resident_servers, start_of, unit_of,
};
pub use value_objects::{Listener, ServerKind, SettingName};

use std::path::{Path, PathBuf};

use crate::collectors::canonical_tool::CanonicalTool;

// One import, because `rastro-collector` re-exports what an author needs.
use rastro_collector::{
    ClaimQualifier, CollectionError, Collector, CollectorCategory, CollectorId, CollectorIdentity,
    CollectorVersion, FacetName, FilesystemClaim, Observation, Presence, WalkedTree,
};

/// Where the kernel publishes its process table.
const PROC: &str = "/proc";

/// The tool that says how a unit starts its server.
const SYSTEMCTL: &str = "systemctl";

pub struct RedisCollector {
    name: FacetName,
    identity: CollectorIdentity,
    installed: InstalledServers,
    proc: PathBuf,

    /// Asked only about a server that refused to answer without a password, for the command
    /// that started it; see [`password_for`].
    systemctl: Option<CanonicalTool>,
}

impl RedisCollector {
    pub fn new() -> Self {
        let collector = Self::reading(InstalledServers::located(), Path::new(PROC));

        match CanonicalTool::located(SYSTEMCTL) {
            Some(systemctl) => collector.asking_systemd(systemctl),
            None => collector,
        }
    }

    /// The same collector over sources the caller chose.
    pub fn reading(installed: InstalledServers, proc: &Path) -> Self {
        Self {
            name: FacetName::new("redis").expect("`redis` is a legal facet name"),
            identity: CollectorIdentity::new(
                CollectorId::new("redis").expect("`redis` is a legal collector id"),
                CollectorVersion::new("1").expect("`1` is a legal collector version"),
            ),
            installed,
            proc: proc.to_path_buf(),
            systemctl: None,
        }
    }

    /// The same, able to ask systemd how a password-protected server was started.
    pub fn asking_systemd(mut self, systemctl: CanonicalTool) -> Self {
        self.systemctl = Some(systemctl);
        self
    }
}

impl Default for RedisCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl Collector for RedisCollector {
    fn name(&self) -> &FacetName {
        &self.name
    }

    fn identity(&self) -> &CollectorIdentity {
        &self.identity
    }

    fn category(&self) -> CollectorCategory {
        CollectorCategory::State
    }

    /// `present` where a server is installed **or** running, `absent` only where neither.
    ///
    /// A server running from `/opt` with no binary in a system directory is still a server, and
    /// the rabbitmq facet learnt the hard way that presence read from the binary alone reports
    /// such a box as having none. Neither answer is `Undetermined`: installed-and-stopped and
    /// not-installed are different facts, and what rastro cannot read surfaces from
    /// [`Collector::collect`] as an `error`.
    fn presence(&self) -> Presence {
        let running = !resident_servers(&self.proc).is_empty();

        match running || !self.installed.is_empty() {
            true => Presence::Present,
            false => Presence::Absent,
        }
    }

    /// Each server's data directory, sealed.
    ///
    /// **Sealed rather than merely unhashed**, for the reason the postgres cluster directory and
    /// the rabbitmq store are: every attribute of the dump moves whenever the server saves, which
    /// its `save` rules decide and nobody touching the box does, and a fingerprint of an unchanged
    /// host must be byte-identical. What the server holds, this facet reports from the server.
    ///
    /// One claim per instance, qualified by the instance's key, so a directory two servers share
    /// says which two. A key a qualifier cannot hold still gets its directory sealed: losing which
    /// instance asked costs a label, and losing the claim would put a live dump under the walk.
    fn filesystem_claims(&self) -> Vec<FilesystemClaim> {
        discover(&self.proc)
            .into_iter()
            .flat_map(|server| {
                data_directories_of(&self.proc, server.process_id)
                    .into_iter()
                    .filter_map(move |directory| {
                        let tree = WalkedTree::new(directory.to_str()?).ok()?;
                        let sealed = FilesystemClaim::sealed(tree);

                        Some(match ClaimQualifier::new(server.key.clone()) {
                            Ok(qualifier) => sealed.for_entry(qualifier),
                            Err(_) => sealed,
                        })
                    })
            })
            .collect()
    }

    /// Never an `error` as a whole: what could not be read of a server is that instance's own
    /// `error`, so one unreadable server does not cost the others.
    fn collect(&self) -> Result<Observation, CollectionError> {
        Ok(Observation::from(&read_installation(
            &self.proc,
            &self.installed,
            self.systemctl.as_ref(),
        )))
    }
}
