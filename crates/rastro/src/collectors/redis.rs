//! Layer 3: what a redis or valkey server is actually running with.
//!
//! **The running server is the only honest account of itself.** On the estate that motivated
//! this facet, `maxmemory` and `save` are applied with `CONFIG SET` and never written to a file,
//! so the configuration on disk and the running server disagree by design and no walk of the
//! filesystem can see it. The facet asks the server.
pub mod model;
pub mod source;
pub mod value_objects;

pub use model::Installation;
pub use source::{
    DialTarget, DiscoveredServer, InstalledServers, Reply, ResidentServer, RespConnection,
    ServerStream, discover, resident_servers,
};
pub use value_objects::{Listener, ServerKind};

use std::path::{Path, PathBuf};

// One import, because `rastro-collector` re-exports what an author needs.
use rastro_collector::{
    CollectionError, Collector, CollectorCategory, CollectorId, CollectorIdentity,
    CollectorVersion, FacetName, Observation, Presence,
};

/// Where the kernel publishes its process table.
const PROC: &str = "/proc";

pub struct RedisCollector {
    name: FacetName,
    identity: CollectorIdentity,
    installed: InstalledServers,
    proc: PathBuf,
}

impl RedisCollector {
    pub fn new() -> Self {
        Self::reading(InstalledServers::located(), Path::new(PROC))
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
        }
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

    fn collect(&self) -> Result<Observation, CollectionError> {
        if !resident_servers(&self.proc).is_empty() {
            return Err(CollectionError::new(
                "a redis server is running and reading one is not built yet",
            ));
        }

        let installation = Installation {
            installed: self.installed.kinds().clone(),
        };

        Ok(Observation::from(&installation))
    }
}
