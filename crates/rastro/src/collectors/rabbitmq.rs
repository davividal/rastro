//! Layer 3: what a RabbitMQ node is actually running with.
//!
//! **Nothing is asked speculatively, and that is the facet's first rule rather than a
//! precaution.** A RabbitMQ CLI tool is not a client that opens a socket: it boots an Erlang
//! VM and joins the broker's distribution cluster, and a call that fails because no node is
//! there still leaves an `epmd -daemon` running on a box that had none. Measured, twice, as
//! root and as the broker's own user. So the dispatch starts from what is already resident:
//! epmd in the process list, then the register it keeps, then a CLI tool addressed at a node
//! the register named. See `docs/decisions.md`.
pub mod model;
pub mod source;
pub mod value_objects;

pub use model::{
    Alarm, Binding, Definitions, Exchange, Installation, Listener, Node, NodeStatus, Parameter,
    Permission, Policy, Queue, TopicPermission, User, UserLimit, Vhost,
};
pub use source::{
    BrokerClient, EpmdRegister, NodeInventory, RabbitmqctlDefinitions, RabbitmqctlFeatureFlags,
    RabbitmqctlStatus, RegisteredNode, ResidentRuntime, document_in,
};
pub use value_objects::{BrokerEvidence, DefinitionValue, NodeName, NotAsked, PasswordHashing};

// One import, because `rastro-collector` re-exports what an author needs.
use rastro_collector::{
    ClaimQualifier, CollectionError, Collector, CollectorCategory, CollectorId, CollectorIdentity,
    CollectorVersion, Concurrency, FacetName, FilesystemClaim, Observation, Presence, WalkedTree,
};

pub struct RabbitmqCollector {
    name: FacetName,
    identity: CollectorIdentity,

    /// The CLI tool, located but not yet run.
    ///
    /// Holding it is what makes presence and a later read agree about which binary they are
    /// talking about, the way every other shelling collector does. Nothing invokes it until a
    /// node has been named *and* attributed to a RabbitMQ process, which is the facet's whole
    /// gate.
    client: Option<BrokerClient>,

    inventory: Option<NodeInventory>,
}

impl RabbitmqCollector {
    pub fn new() -> Self {
        Self::reading(BrokerClient::located(), NodeInventory::detect())
    }

    /// The same collector over sources the caller chose.
    pub fn reading(client: Option<BrokerClient>, inventory: Option<NodeInventory>) -> Self {
        Self {
            name: FacetName::new("rabbitmq").expect("`rabbitmq` is a legal facet name"),
            identity: CollectorIdentity::new(
                CollectorId::new("rabbitmq").expect("`rabbitmq` is a legal collector id"),
                CollectorVersion::new("1").expect("`1` is a legal collector version"),
            ),
            client,
            inventory,
        }
    }
}

impl Default for RabbitmqCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl Collector for RabbitmqCollector {
    fn name(&self) -> &FacetName {
        &self.name
    }

    fn identity(&self) -> &CollectorIdentity {
        &self.identity
    }

    fn category(&self) -> CollectorCategory {
        CollectorCategory::State
    }

    /// `present` where RabbitMQ is installed **or** running, `absent` only where neither.
    ///
    /// **A running broker is not hidden by a missing client**, which an earlier version did:
    /// presence was the client alone, so a box with a broker up and no `rabbitmqctl` on it
    /// reported no RabbitMQ at all, and the inventory's own clientless path became
    /// unreachable while a test went on passing over it. The register, the port mapper and
    /// the broker processes are all readable without a client, and what they say is worth
    /// the facet.
    ///
    /// Neither answer is `Undetermined`: installed-and-stopped and not-installed are
    /// different facts about the host rather than two ways of failing to look, and the
    /// reasons rastro genuinely cannot look surface from
    /// [`Collector::collect`] as an `error` instead.
    fn presence(&self) -> Presence {
        let installed = self.client.is_some();
        let running = self
            .inventory
            .as_ref()
            .is_some_and(NodeInventory::brokers_resident);

        match installed || running {
            true => Presence::Present,
            false => Presence::Absent,
        }
    }

    /// Alone, like the walk, and for a neighbouring reason.
    ///
    /// Every read of a node boots an Erlang VM that joins the broker's distribution cluster,
    /// which binds a port for as long as the call lasts. Shared, that ephemeral listener and
    /// its `beam.smp` race the `sockets` and `processes` collectors reading the same box, so
    /// which of them a run records is decided by thread scheduling and two runs of an
    /// unchanged host differ. The walk declares itself exclusive because it would notice
    /// another collector's temp file; this one declares itself exclusive because it *is* the
    /// thing another collector would notice.
    fn concurrency(&self) -> Concurrency {
        Concurrency::Exclusive
    }

    /// Each node's store, sealed.
    ///
    /// **Sealed rather than merely unhashed**, the strongest claim in the vocabulary, for the
    /// reasons the postgres cluster directory gets it: on a real broker it is most of the
    /// files, every attribute the walk would record moves on the next write, and a fingerprint
    /// whose contract is that two runs of an unchanged box are byte-identical cannot carry a
    /// tree that rewrites itself. Measured with no client connected and nothing published:
    /// three files under it moved in ten idle seconds.
    ///
    /// What is actually in there, the vhosts, the users, the permissions, the policies and the
    /// topology, this facet reports properly, from the node rather than from its files. The
    /// root entry stays, so a reader still sees the directory, its mode and its owner, and the
    /// effective table in the `invocation` facet names this facet as the reason nothing is
    /// under it.
    ///
    /// One claim per node, each naming the node it was made for, because a box legitimately
    /// runs several and a directory two of them point at should say which two.
    fn filesystem_claims(&self) -> Vec<FilesystemClaim> {
        let Some(inventory) = &self.inventory else {
            return Vec::new();
        };

        inventory
            .store_directories()
            .into_iter()
            .filter_map(|(node, directory)| {
                let tree = WalkedTree::new(directory).ok()?;
                let sealed = FilesystemClaim::sealed(tree);

                // A node whose name cannot be a qualifier still gets its store sealed. Losing
                // which node asked costs precision in a report; losing the claim would put a
                // live message store back under the walk.
                Some(match ClaimQualifier::new(node.as_str()) {
                    Ok(qualifier) => sealed.for_entry(qualifier),
                    Err(_) => sealed,
                })
            })
            .collect()
    }

    fn collect(&self) -> Result<Observation, CollectionError> {
        let inventory = self.inventory.as_ref().ok_or_else(|| {
            CollectionError::new(
                "no epmd was found in a system directory, so the nodes on this box cannot be \
                 named without starting one",
            )
        })?;

        Ok(Observation::from(&inventory.read(self.client.as_ref())?))
    }
}
