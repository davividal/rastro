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
pub mod model;
pub mod source;
pub mod value_objects;

pub use model::{ListeningSocket, SocketAddress, SocketProcess, SocketTable};
pub use source::{Ss, ss_address, ss_users};
pub use value_objects::{
    InetHost, InterfaceScope, PortNumber, ProcessName, SocketKind, SocketPath, SocketState,
};

// One import, because `rastro-collector` re-exports what an author needs. A
// collector written outside this repo looks exactly like this.
use rastro_collector::{
    CollectionError, Collector, CollectorCategory, CollectorId, CollectorIdentity,
    CollectorVersion, FacetName, Observation, Presence,
};

pub struct SocketsCollector {
    name: FacetName,
    identity: CollectorIdentity,
    ss: Option<Ss>,
}

impl SocketsCollector {
    pub fn new() -> Self {
        Self::reading(Ss::detect())
    }

    /// The same collector over a source the caller chose.
    pub fn reading(ss: Option<Ss>) -> Self {
        Self {
            name: FacetName::new("sockets").expect("`sockets` is a legal facet name"),
            identity: CollectorIdentity::new(
                CollectorId::new("sockets").expect("`sockets` is a legal collector id"),
                CollectorVersion::new("1").expect("`1` is a legal collector version"),
            ),
            ss,
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

    /// `undetermined` without `ss`, which is neither of the answers the other collectors
    /// give, and the difference is worth stating.
    ///
    /// A box with no `ss` has not stopped listening on anything: the sockets are there and
    /// rastro cannot see them, which is exactly what `undetermined` means and exactly what
    /// `absent` would deny. That is the opposite of the units collector, where a missing
    /// `systemctl` really does mean there are no systemd units, and of the packages
    /// collector, where rastro reports what it can read and says nothing about the rest.
    ///
    /// It reaches the document as `error` with this reason, which is right: a fingerprint
    /// silently missing the box's exposed ports would be worse than a loud one.
    fn presence(&self) -> Presence {
        match self.ss {
            Some(_) => Presence::Present,
            None => Presence::Undetermined {
                reason: "`ss` was not found, so what this host is listening on cannot be told"
                    .to_owned(),
            },
        }
    }

    fn collect(&self) -> Result<Observation, CollectionError> {
        let ss = self
            .ss
            .as_ref()
            .ok_or_else(|| CollectionError::new("`ss` was not found"))?;

        Ok(Observation::from(&ss.read()?))
    }
}
