//! The nodes on the box, and which of them may be asked anything.
//!
//! The one place the facet's gate is enforced: the register is read only where the port
//! mapper is already running, because `epmd -names` is safe to run and a CLI tool is not, and
//! the whole point of asking epmd first is to avoid starting one.

use std::path::{Path, PathBuf};

use rastro_collector::CollectionError;

use super::broker_client::BrokerClient;
use super::epmd_register::EpmdRegister;
use super::node_layout;
use super::resident_runtime::ResidentRuntime;
use crate::collectors::canonical_tool::CanonicalTool;
use crate::collectors::proc_sockets::{SocketHolders, listening_inodes};
use crate::collectors::rabbitmq::model::{Definitions, Installation, Node, NodeStatus};
use crate::collectors::rabbitmq::value_objects::{BrokerEvidence, NodeName};

/// The program that keeps the register of Erlang nodes.
const REGISTER_PROGRAM: &str = "epmd";

/// The argument that prints the register, and the only argument rastro ever gives it.
const NAMES: &str = "-names";

/// What a CLI tool calls its own hidden node, which is in the register while it runs.
///
/// **Measured, because a conformance run caught one**: with the register sampled continuously
/// while six `rabbitmqctl` calls ran, it held `name rabbitmqcli-308-rabbit at port 35672`, and
/// nothing but the broker once they finished. CI saw the same thing from the other side, the
/// facet reporting `["rabbit", "rabbitmqcli-819-rabbit"]` as two nodes.
///
/// **Filtered by name, which is the tool's own naming rather than a guess**: `rabbitmqcli-`
/// then the caller's process id then the node it is addressing. The alternative is to report
/// it, and that cannot be right: the entry exists only while somebody is running a CLI tool,
/// so two runs of an unchanged box would differ, which the document's contract forbids.
///
/// **Being exclusive does not cover this.** rastro's own calls are sequenced, but an operator
/// at a shell, a monitoring script or a deployment can be running `rabbitmqctl` at the moment
/// the register is read, and on a busy box that is not a remote possibility.
const CLI_NODE_PREFIX: &str = "rabbitmqcli-";

/// Where the kernel publishes its process table, and its socket tables under it.
const PROC: &str = "/proc";
const NET: &str = "net";

/// The register and the process table, which between them say what is here.
///
/// **No hostname.** An earlier version carried the box's name so it could compose
/// `local@host`; the node's own name is read from the files it holds open instead, so nothing
/// about the box needs to be guessed at.
pub struct NodeInventory {
    port_mapper: CanonicalTool,
    proc: PathBuf,
}

impl NodeInventory {
    /// The inventory of this box, where a port mapper is installed at all.
    pub fn detect() -> Option<Self> {
        CanonicalTool::located(REGISTER_PROGRAM).map(Self::using)
    }

    /// The same over a register the caller located, which is what makes this testable.
    pub fn using(port_mapper: CanonicalTool) -> Self {
        Self {
            port_mapper,
            proc: PathBuf::from(PROC),
        }
    }

    /// The same over a process table the caller names.
    pub fn in_proc(mut self, proc: &Path) -> Self {
        self.proc = proc.to_path_buf();
        self
    }

    /// Whether a process on this box booted RabbitMQ.
    ///
    /// What [`presence`](rastro_collector::Collector::presence) needs, and a `/proc` walk
    /// rather than a question put to anybody: a broker running with no client installed to
    /// ask it with is still a broker, and reporting that box as having no RabbitMQ would be a
    /// confident lie about it.
    pub fn brokers_resident(&self) -> bool {
        !ResidentRuntime::read_in(&self.proc)
            .broker_process_ids()
            .is_empty()
    }

    /// What this box holds, asking epmd only where epmd is already running.
    ///
    /// **A box with no port mapper is not a failure and not a question.** It is an
    /// installation with nothing up, reported from the process table alone, and the register
    /// is not touched.
    ///
    /// A resident port mapper that will not answer *is* a failure, because rastro cannot tell
    /// it apart from a box whose nodes it failed to find.
    pub fn read(&self, client: Option<&BrokerClient>) -> Result<Installation, CollectionError> {
        let resident = ResidentRuntime::read_in(&self.proc);
        let brokers = resident.broker_process_ids().len();

        if !resident.port_mapper_running() {
            return Ok(Installation::new(false, brokers, []));
        }

        let registered = EpmdRegister::parse(&self.port_mapper.run(&[NAMES])?)?;
        let holders = SocketHolders::at(&self.proc);
        let nodes = registered
            .into_iter()
            .filter(|node| !is_a_cli_tool(&node.name))
            .map(|node| {
                let evidence = self.evidence_for(node.distribution_port, &holders, &resident);
                let name = self.named(&node.name, &resident);

                // Asked only where a RabbitMQ process holds the port, where the node's own
                // name could be read, and where there is a client to ask with. A node rastro
                // cannot name is one it cannot address either: `rabbitmqctl -n` takes the
                // name the node runs under, and guessing it is what this facet stopped doing.
                let asked = match (evidence.may_be_addressed(), &name, client) {
                    (true, Some(name), Some(client)) => Some(Asked {
                        status: client.status(name)?,
                        feature_flags: client.feature_flags(name)?,
                        definitions: client.definitions(name)?,
                    }),
                    _ => None,
                };

                Ok((
                    node.name,
                    Node {
                        node_name: name,
                        distribution_port: node.distribution_port,
                        evidence,
                        status: asked.as_ref().map(|asked| asked.status.clone()),
                        feature_flags: asked.as_ref().map(|asked| asked.feature_flags.clone()),
                        definitions: asked.map(|asked| asked.definitions),
                    },
                ))
            })
            .collect::<Result<Vec<_>, CollectionError>>()?;

        Ok(Installation::new(true, brokers, nodes))
    }

    /// The store each node keeps, for the trees the facet claims.
    ///
    /// **The claim phase, which is not the collect phase.** Claims are gathered before any
    /// collector runs and before the walk, sequentially, so this is on the critical path of
    /// every run and pays for nothing it does not need: the register, which is 2 ms and was
    /// measured not to start the daemon it may fail to reach, and then `/proc`. The broker is
    /// never asked, because it writes where its store is into the paths it holds open.
    ///
    /// **A failed read makes no claim**, which is the postgres rule and for its reason.
    pub fn store_directories(&self) -> Vec<(NodeName, String)> {
        let resident = ResidentRuntime::read_in(&self.proc);
        if !resident.port_mapper_running() {
            return Vec::new();
        }

        let Ok(listed) = self.port_mapper.run(&[NAMES]) else {
            return Vec::new();
        };

        let Ok(registered) = EpmdRegister::parse(&listed) else {
            return Vec::new();
        };

        registered
            .into_iter()
            .filter(|node| !is_a_cli_tool(&node.name))
            .filter_map(|node| {
                let layout =
                    node_layout::read(&self.proc, resident.broker_process_ids(), &node.name)?;
                let name = NodeName::parse(layout.name).ok()?;

                Some((name, layout.store))
            })
            .collect()
    }

    /// The node's own name, where its files say what it is.
    fn named(&self, registered: &str, resident: &ResidentRuntime) -> Option<NodeName> {
        let layout = node_layout::read(&self.proc, resident.broker_process_ids(), registered)?;

        NodeName::parse(layout.name).ok()
    }

    /// What the box's own evidence says about this node, without addressing it.
    ///
    /// **Three reads, none of which asks anything of anybody.** The socket table says which
    /// inodes are offered on the port, the descriptor walk says which processes hold those
    /// inodes, and the process table has already said which processes booted RabbitMQ. The
    /// intersection is the answer, and it is the whole reason this facet can address a node
    /// without first poking every node in the register to find out what it is.
    ///
    /// **Each way of not knowing is kept apart from the others**, because they are different
    /// facts about the box. A first version folded them into `false` and a live broker in a
    /// capability-reduced container reported `runs_rabbitmq: false`: rastro could not read the
    /// beam's descriptors, so it could not see who held the port. The behaviour was right and
    /// the report was wrong, which is the worse of the two failures.
    fn evidence_for(
        &self,
        port: u16,
        holders: &SocketHolders,
        resident: &ResidentRuntime,
    ) -> BrokerEvidence {
        let Some(inodes) = listening_inodes(&self.proc.join(NET), port) else {
            return BrokerEvidence::TablesUnreadable;
        };

        if inodes.is_empty() {
            return BrokerEvidence::NotOffered;
        }

        let holding: Vec<i64> = inodes
            .into_iter()
            .flat_map(|inode| holders.process_ids_of(inode))
            .collect();

        if holding.is_empty() {
            return BrokerEvidence::HolderUnreadable;
        }

        let booted_rabbit = holding.iter().any(|held| {
            resident
                .broker_process_ids()
                .iter()
                .any(|broker| i64::from(*broker) == *held)
        });

        match booted_rabbit {
            true => BrokerEvidence::RabbitmqProcess,
            false => BrokerEvidence::OtherApplication,
        }
    }
}

/// Whether a registration belongs to a CLI tool rather than to a broker.
fn is_a_cli_tool(name: &str) -> bool {
    name.starts_with(CLI_NODE_PREFIX)
}

/// What a node answered, kept together so a half-read node cannot be assembled.
///
/// Both reads or neither: a node carrying a status and no definitions would read as a broker
/// with no vhosts at all, which is a state RabbitMQ cannot be in.
struct Asked {
    status: NodeStatus,
    feature_flags: std::collections::BTreeMap<String, String>,
    definitions: Definitions,
}
