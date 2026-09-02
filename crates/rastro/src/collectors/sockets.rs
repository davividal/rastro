//! What this box is listening on.
//!
//! Three layers, and the dependency arrows only point one way:
//! [`source`] knows [`model`], `model` knows [`value_objects`], and neither of the last
//! two knows a host interface exists.
//!
//! **The facet that answers "what can reach this box".** A package list says what is
//! installed and the units facet says what is running; this says which of it is reachable,
//! and from where. `*:9100` and `127.0.0.1:9100` are one character apart in a
//! configuration file and the difference between an exporter exposed to the network and
//! one exposed to nothing.
//!
//! Both socket families are read. Unix sockets matter for a reason that is easy to miss:
//! an *abstract* one has a name in no filesystem at all, so a filesystem walk cannot see
//! it and this is the only facet that records it.
//!
//! **Read from `/proc`, not from `ss`, because asking `ss` changes the box.** See
//! [`ProcNet`] for what that costs and what it does not.
pub mod model;
pub mod source;
pub mod value_objects;

pub use model::{ListeningSocket, SocketAddress, SocketProcess, SocketTable};
pub use source::{InetTable, ProcNet, SocketHolders, SocketRow, proc_net_inet, proc_net_unix};
pub use value_objects::{InetHost, PortNumber, ProcessName, SocketKind, SocketPath, SocketState};

// One import, because `rastro-collector` re-exports what an author needs. A
// collector written outside this repo looks exactly like this.
use rastro_collector::{
    CollectionError, Collector, CollectorCategory, CollectorId, CollectorIdentity,
    CollectorVersion, FacetName, Observation, Presence,
};

pub struct SocketsCollector {
    name: FacetName,
    identity: CollectorIdentity,
    source: Option<ProcNet>,
}

impl SocketsCollector {
    pub fn new() -> Self {
        Self::reading(ProcNet::detect())
    }

    /// The same collector over a source the caller chose.
    pub fn reading(source: Option<ProcNet>) -> Self {
        Self {
            name: FacetName::new("sockets").expect("`sockets` is a legal facet name"),
            identity: CollectorIdentity::new(
                CollectorId::new("sockets").expect("`sockets` is a legal collector id"),
                CollectorVersion::new("2").expect("`2` is a legal collector version"),
            ),
            source,
        }
    }
}

impl Default for SocketsCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl Collector for SocketsCollector {
    fn name(&self) -> &FacetName {
        &self.name
    }

    fn identity(&self) -> &CollectorIdentity {
        &self.identity
    }

    fn category(&self) -> CollectorCategory {
        CollectorCategory::State
    }

    /// `undetermined` without a procfs, which is neither of the answers the other
    /// collectors give, and the difference is worth stating.
    ///
    /// A box rastro cannot read `/proc/net` on has not stopped listening on anything: the
    /// sockets are there and rastro cannot see them, which is exactly what `undetermined`
    /// means and exactly what `absent` would deny. That is the opposite of the units
    /// collector, where a missing `systemctl` really does mean there are no systemd units,
    /// and of the packages collector, where rastro reports what it can read and says
    /// nothing about the rest.
    ///
    /// It reaches the document as `error` with this reason, which is right: a fingerprint
    /// silently missing the box's exposed ports would be worse than a loud one.
    fn presence(&self) -> Presence {
        match self.source {
            Some(_) => Presence::Present,
            None => Presence::Undetermined {
                reason: "/proc/net/unix was not found, so what this host is listening on \
                         cannot be told"
                    .to_owned(),
            },
        }
    }

    fn collect(&self) -> Result<Observation, CollectionError> {
        let source = self
            .source
            .as_ref()
            .ok_or_else(|| CollectionError::new("/proc/net/unix was not found"))?;

        Ok(Observation::from(&source.read()?))
    }
}
