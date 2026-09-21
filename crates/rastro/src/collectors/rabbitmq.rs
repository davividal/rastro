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
    Binding, Definitions, Exchange, Installation, Listener, Node, NodeStatus, Parameter,
    Permission, Policy, Queue, TopicPermission, User, UserLimit, Vhost,
};
pub use source::{
    BrokerClient, EpmdRegister, NodeInventory, RabbitmqctlDefinitions, RabbitmqctlStatus,
    RegisteredNode, ResidentRuntime,
};
pub use value_objects::{DefinitionValue, NodeName, PasswordHashing};

// One import, because `rastro-collector` re-exports what an author needs.
use rastro_collector::{
    CollectionError, Collector, CollectorCategory, CollectorId, CollectorIdentity,
    CollectorVersion, FacetName, Observation, Presence,
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
    pub fn new(hostname: Result<String, String>) -> Self {
        Self::reading(BrokerClient::located(), NodeInventory::detect(hostname))
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

    /// `absent` without `rabbitmqctl`, `present` with it even where no node is running.
    ///
    /// The two are different facts and the document keeps them apart: a box with the CLI and
    /// nothing up has had RabbitMQ installed and stopped, which is state, while a box without
    /// it has no RabbitMQ at all. Neither is a failure, so neither is `Undetermined`: with
    /// the CLI installed there is always something to read, because the process table and the
    /// register are readable whatever the broker is doing.
    fn presence(&self) -> Presence {
        match self.client {
            Some(_) => Presence::Present,
            None => Presence::Absent,
        }
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
