//! The `rabbitmqctl` interface: the two reads a node is asked for.
//!
//! **Addressed at a node by name, always.** Without `-n` the tool talks to whatever
//! `RABBITMQ_NODENAME` or the default says, which on a box running two nodes is a coin toss
//! about which one a facet describes. The name comes from the register, so what is asked and
//! what is reported cannot disagree.
//!
//! **Run as rastro's own account.** Measured on Debian 13: root answers while holding no
//! cookie of its own, and the broker's own user answers too. Where rastro is neither, the
//! read fails and the facet says so, naming the requirement, because the tool's own
//! diagnosis for that case is a usage dump that would tell an operator nothing. Dropping to
//! the broker's account through [`ToolAsUser`](crate::collectors::canonical_tool::ToolAsUser)
//! is the documented fallback and is not built: it needs the account behind a process id,
//! which is a passwd lookup this facet has no business owning yet.

use rastro_collector::CollectionError;

use crate::collectors::canonical_tool::CanonicalTool;
use crate::collectors::rabbitmq::model::{Definitions, NodeStatus};
use crate::collectors::rabbitmq::source::{RabbitmqctlDefinitions, RabbitmqctlStatus};
use crate::collectors::rabbitmq::value_objects::NodeName;

/// The client whose presence says RabbitMQ was installed here.
const PROGRAM: &str = "rabbitmqctl";

/// The flag that names the node, and the formatter that makes an answer parseable.
const NODE_FLAG: &str = "-n";
const FORMATTER: &str = "--formatter";
const JSON: &str = "json";

/// The read that asks a node what it is running with.
const STATUS: &str = "status";

/// The read that asks a node for its durable definitions.
///
/// `-` is the whole difference between a read and a write: given a path instead, this command
/// creates the file.
const EXPORT: &str = "export_definitions";
const STDOUT: &str = "-";

/// A located `rabbitmqctl`, ready to be addressed at a node.
pub struct BrokerClient {
    tool: CanonicalTool,
}

impl BrokerClient {
    /// The client on this box, where one is installed.
    pub fn located() -> Option<Self> {
        CanonicalTool::located(PROGRAM).map(|tool| Self { tool })
    }

    /// The same over a tool the caller located, which is what makes the reads testable.
    pub fn using(tool: CanonicalTool) -> Self {
        Self { tool }
    }

    /// The tool itself, for a caller that needs to say which binary answered.
    pub fn tool(&self) -> &CanonicalTool {
        &self.tool
    }

    /// What the node says it is running with.
    pub fn status(&self, node: &NodeName) -> Result<NodeStatus, CollectionError> {
        let answer = self
            .tool
            .run(&[NODE_FLAG, node.as_str(), STATUS, FORMATTER, JSON])?;

        RabbitmqctlStatus::parse(&answer)
    }

    /// The durable half of the node: its tenancy, accounts and topology.
    pub fn definitions(&self, node: &NodeName) -> Result<Definitions, CollectionError> {
        let answer = self.tool.run(&[NODE_FLAG, node.as_str(), EXPORT, STDOUT])?;

        RabbitmqctlDefinitions::parse(&answer)
    }
}
