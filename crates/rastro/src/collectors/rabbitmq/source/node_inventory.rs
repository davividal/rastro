//! Composing the nodes to ask, from what is resident and what the register holds.
//!
//! The one place the facet's gate is enforced: the register is read only where the port
//! mapper is already running, because `epmd -names` is safe to run and a CLI tool is not,
//! and the whole point of asking epmd first is to avoid starting one.

use std::path::{Path, PathBuf};

use rastro_collector::CollectionError;

use super::broker_client::BrokerClient;
use super::epmd_register::EpmdRegister;
use super::resident_runtime::ResidentRuntime;
use crate::collectors::canonical_tool::CanonicalTool;
use crate::collectors::proc_sockets::{SocketHolders, listening_inodes};
use crate::collectors::rabbitmq::model::{Definitions, Installation, Node, NodeStatus};
use crate::collectors::rabbitmq::value_objects::NodeName;

/// The program that keeps the register of Erlang nodes.
const REGISTER_PROGRAM: &str = "epmd";

/// The argument that prints the register, and the only argument rastro ever gives it.
const NAMES: &str = "-names";

/// Where the kernel publishes its process table, and its socket tables under it.
const PROC: &str = "/proc";
const NET: &str = "net";

/// The register, the process table, and the host whose nodes they describe.
///
/// **The gate lives in this one type on purpose.** Residency and the register are two reads
/// that only mean something together: the first says whether the second may be asked, and a
/// caller holding them apart could ask in the wrong order. So the caller gets one `read`
/// and no way to skip the order.
pub struct NodeInventory {
    port_mapper: CanonicalTool,

    /// The hostname the run resolved, or why it could not be.
    ///
    /// Carried as the run left it, the way `HostCollector` carries it: a node is addressed as
    /// `local@host`, so a box that cannot say what it is called cannot have its nodes named,
    /// and that is a loud failure rather than a guessed key.
    hostname: Result<String, String>,

    proc: PathBuf,
}

impl NodeInventory {
    /// The inventory of this box, where a port mapper is installed at all.
    pub fn detect(hostname: Result<String, String>) -> Option<Self> {
        CanonicalTool::located(REGISTER_PROGRAM).map(|port_mapper| Self {
            port_mapper,
            hostname,
            proc: PathBuf::from(PROC),
        })
    }

    /// The same over a register the caller located, which is what makes this testable.
    pub fn using(port_mapper: CanonicalTool, hostname: Result<String, String>) -> Self {
        Self {
            port_mapper,
            hostname,
            proc: PathBuf::from(PROC),
        }
    }

    /// The same over a process table the caller names.
    pub fn in_proc(mut self, proc: &Path) -> Self {
        self.proc = proc.to_path_buf();
        self
    }

    /// What this box holds, asking epmd only where epmd is already running.
    ///
    /// **A box with no port mapper is not a failure and not a question.** It is an
    /// installation with nothing up, reported from the process table alone, and the register
    /// is not touched: asking it would start nothing, but it is also the step that decides
    /// whether the rest of the facet asks anything at all, and that decision belongs where
    /// the evidence is.
    ///
    /// A resident port mapper that will not answer *is* a failure, because rastro cannot
    /// tell it apart from a box whose nodes it failed to find.
    pub fn read(&self, client: Option<&BrokerClient>) -> Result<Installation, CollectionError> {
        let resident = ResidentRuntime::read_in(&self.proc);
        let brokers = resident.broker_process_ids().len();

        if !resident.port_mapper_running() {
            return Ok(Installation::new(false, brokers, []));
        }

        let hostname = self.hostname.as_ref().map_err(|reason| {
            CollectionError::new(format!(
                "a node is addressed as local@host and this box could not say what it is \
                 called, so its nodes cannot be named: {reason}"
            ))
        })?;

        let registered = EpmdRegister::parse(&self.port_mapper.run(&[NAMES])?)?;
        let holders = SocketHolders::at(&self.proc);
        let nodes = registered
            .into_iter()
            .map(|node| {
                let name = NodeName::new(node.name, hostname.as_str())?;
                let runs_rabbitmq = self.is_broker(node.distribution_port, &holders, &resident);

                // Asked only where a RabbitMQ process holds the port, and only with a client
                // to ask with. Every other case is a node reported as what the box knows of
                // it, which is the whole restraint this facet is arranged around.
                let asked = match (runs_rabbitmq, client) {
                    (true, Some(client)) => Some(Asked {
                        status: client.status(&name)?,
                        definitions: client.definitions(&name)?,
                    }),
                    _ => None,
                };

                Ok((
                    name,
                    Node {
                        distribution_port: node.distribution_port,
                        runs_rabbitmq,
                        status: asked.as_ref().map(|asked| asked.status.clone()),
                        definitions: asked.map(|asked| asked.definitions),
                    },
                ))
            })
            .collect::<Result<Vec<_>, CollectionError>>()?;

        Ok(Installation::new(true, brokers, nodes))
    }

    /// Whether a process that booted RabbitMQ is the one offering this node's port.
    ///
    /// **Two reads, and neither of them addresses anything.** The socket table says which
    /// inodes are offered on the port, the descriptor walk says which processes hold those
    /// inodes, and the process table has already said which processes booted RabbitMQ. The
    /// intersection is the answer, and it is the whole reason this facet can address a node
    /// without first poking every node in the register to find out what it is.
    ///
    /// **False is the safe answer and it is reached by three different host states**: another
    /// Erlang application holding the port, a stale registration whose process is gone, and
    /// an unprivileged run that cannot read another user's descriptors. All three mean the
    /// same thing here, which is that rastro has no evidence it is talking to a broker and
    /// therefore does not talk.
    fn is_broker(&self, port: u16, holders: &SocketHolders, resident: &ResidentRuntime) -> bool {
        listening_inodes(&self.proc.join(NET), port)
            .into_iter()
            .flat_map(|inode| holders.process_ids_of(inode))
            .any(|held| {
                resident
                    .broker_process_ids()
                    .iter()
                    .any(|broker| i64::from(*broker) == held)
            })
    }
}

/// What a node answered, kept together so a half-read node cannot be assembled.
///
/// Both reads or neither: a node carrying a status and no definitions would read as a broker
/// with no vhosts at all, which is a state RabbitMQ cannot be in.
struct Asked {
    status: NodeStatus,
    definitions: Definitions,
}
