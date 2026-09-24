//! The nodes on the box, and which of them may be asked anything.
//!
//! The one place the facet's gate is enforced: the register is read only where the port
//! mapper is already running, because `epmd -names` is safe to run and a CLI tool is not, and
//! the whole point of asking epmd first is to avoid starting one.

use std::path::{Path, PathBuf};

use rastro_collector::CollectionError;

use super::broker_client::BrokerClient;
use super::epmd_register::{EpmdRegister, RegisteredNode};
use super::node_layout;
use super::resident_runtime::ResidentRuntime;
use crate::collectors::canonical_tool::CanonicalTool;
use crate::collectors::proc_sockets::{SocketHolders, listening_inodes};
use crate::collectors::rabbitmq::model::{Definitions, Installation, Node, NodeStatus};
use crate::collectors::rabbitmq::value_objects::NotAsked;
use crate::collectors::rabbitmq::value_objects::{BrokerEvidence, BrokerVersion, FLOOR, NodeName};

/// The program that keeps the register of Erlang nodes.
const REGISTER_PROGRAM: &str = "epmd";

/// The argument that prints the register, and the only argument rastro ever gives it.
const NAMES: &str = "-names";

/// What a CLI tool calls its own hidden node, which is in the register while it runs.
///
/// **Measured, because a conformance run caught one**: with the register sampled continuously
/// while six `rabbitmqctl` calls ran, it held `name rabbitmqcli-308-rabbit at port 35672`, and
/// nothing but the broker once they finished. CI saw the same from the other side, the facet
/// reporting `["rabbit", "rabbitmqcli-819-rabbit"]` as two nodes.
///
/// **The prefix alone is not enough to drop a registration**, which the review caught and a
/// measurement confirmed: `RABBITMQ_NODENAME=rabbitmqcli-legit@localhost` starts a perfectly
/// ordinary broker, epmd lists it as `name rabbitmqcli-legit at port 25672`, and it answers
/// like any other node. Dropping it by name would have hidden a live broker and left its
/// message store unsealed — the same failure the seal exists to prevent.
///
/// So the prefix only raises the question, and [the evidence](BrokerEvidence) answers it: a
/// registration is discarded when it carries this prefix **and** the process holding its port
/// is not one that booted RabbitMQ. A real broker so named is held by a beam that did, and
/// stays.
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

        let Some(listed) = self.registered()? else {
            return Ok(Installation::new(false, brokers, []));
        };

        let holders = SocketHolders::at(&self.proc);
        let nodes = listed
            .into_iter()
            .filter_map(|node| {
                let evidence = self.evidence_for(node.distribution_port, &holders, &resident);

                match discarded_as_a_cli_tool(&node.name, &evidence) {
                    true => None,
                    false => Some((node, evidence)),
                }
            })
            .map(|(node, evidence)| {
                let name = self.named(&node.name, &resident);

                // Asked only where a RabbitMQ process holds the port, where the node's own
                // name could be read, and where there is a client to ask with. A node rastro
                // cannot name is one it cannot address either: `rabbitmqctl -n` takes the
                // name the node runs under, and guessing it is what this facet stopped doing.
                let (asked, not_asked) = match (evidence.may_be_addressed(), &name, client) {
                    (true, Some(name), Some(client)) => (Some(ask(client, name)?), None),
                    (true, None, _) => (None, Some(NotAsked::Nameless)),
                    (true, Some(_), None) => (None, Some(NotAsked::NoClient)),
                    (false, _, _) => (None, None),
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
                        not_asked,
                    },
                ))
            })
            .collect::<Result<Vec<_>, CollectionError>>()?;

        Ok(Installation::new(true, brokers, nodes))
    }

    /// The register, where there is a port mapper to ask.
    ///
    /// `None` means no port mapper is resident, which is a box with nothing up rather than a
    /// failure. One place, because the two callers below read the same thing and an earlier
    /// version spelled it twice: a change that reached one of them and not the other is
    /// exactly how this facet has gone wrong before.
    fn registered(&self) -> Result<Option<Vec<RegisteredNode>>, CollectionError> {
        if !ResidentRuntime::read_in(&self.proc).port_mapper_running() {
            return Ok(None);
        }

        Ok(Some(EpmdRegister::parse(&self.port_mapper.run(&[NAMES])?)?))
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

        // A failure here is nothing rather than an error, which is the one way this differs
        // from the collect-time read: a claim that cannot be made costs a subtree of the
        // walk, and failing the run over it would cost the whole filesystem.
        let Ok(Some(listed)) = self.registered() else {
            return Vec::new();
        };

        // No CLI filter, and none is needed: a layout is looked for among the processes that
        // booted RabbitMQ, and a CLI tool's hidden node matches none of their directories. A
        // broker that happens to carry the prefix does, and is sealed like any other.
        listed
            .into_iter()
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

/// Whether a registration is a CLI tool's own hidden node rather than a broker.
///
/// Both halves are required. The prefix alone would discard a broker legitimately named with
/// it, measured; the evidence alone would keep every transient node on a box where the
/// holders cannot be read, and those nodes are what make two runs of an unchanged box differ.
fn discarded_as_a_cli_tool(name: &str, evidence: &BrokerEvidence) -> bool {
    name.starts_with(CLI_NODE_PREFIX) && *evidence != BrokerEvidence::RabbitmqProcess
}

/// Everything one node is asked.
///
/// **The status comes first so the floor can be checked against it.** It is the cheap read
/// that names the version, and a node below the floor is refused there rather than asked the
/// fat read whose answer this cannot parse. That ordering is not the version branching an
/// earlier release removed: nothing here asks a different question of a different release, it
/// only declines to ask an older one anything more.
fn ask(client: &BrokerClient, node: &NodeName) -> Result<Asked, CollectionError> {
    let status = client.status(node)?;
    supported(&status.rabbitmq_version, node)?;

    Ok(Asked {
        status,
        feature_flags: client.feature_flags(node)?,
        definitions: client.definitions(node)?,
    })
}

/// Refuses a node older than the floor, naming both versions.
///
/// **The message is the point.** A node below the floor answers the reads and then fails the
/// parse somewhere inside a document of several megabytes, which reaches the operator as a
/// byte offset into something they cannot get back. Read against a version instead, the same
/// box says which node, which release, and what rastro reads.
///
/// A version that cannot be ordered at all is left to the reads: it is evidence that whatever
/// answered is not a broker, which is a different failure with its own message.
fn supported(version: &str, node: &NodeName) -> Result<(), CollectionError> {
    let Some(reported) = BrokerVersion::parse(version) else {
        return Ok(());
    };

    if reported < FLOOR {
        return Err(CollectionError::new(format!(
            "{node} runs RabbitMQ {version}, and rastro reads {FLOOR} and newer",
            node = node.as_str()
        )));
    }

    Ok(())
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
