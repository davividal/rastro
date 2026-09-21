//! Composing the nodes to ask, from what is resident and what the register holds.
//!
//! The one place the facet's gate is enforced: the register is read only where the port
//! mapper is already running, because `epmd -names` is safe to run and a CLI tool is not,
//! and the whole point of asking epmd first is to avoid starting one.

use rastro_collector::CollectionError;

use super::epmd_register::EpmdRegister;
use super::resident_runtime::ResidentRuntime;
use crate::collectors::canonical_tool::CanonicalTool;
use crate::collectors::rabbitmq::model::{Installation, Node};
use crate::collectors::rabbitmq::value_objects::NodeName;

/// The program that keeps the register of Erlang nodes.
const REGISTER_PROGRAM: &str = "epmd";

/// The argument that prints the register, and the only argument rastro ever gives it.
const NAMES: &str = "-names";

/// The register, and the host whose nodes it holds.
pub struct NodeInventory {
    port_mapper: CanonicalTool,
    hostname: String,
}

impl NodeInventory {
    /// The inventory of this box, where a port mapper is installed at all.
    pub fn detect(hostname: impl Into<String>) -> Option<Self> {
        CanonicalTool::located(REGISTER_PROGRAM).map(|port_mapper| Self {
            port_mapper,
            hostname: hostname.into(),
        })
    }

    /// The same over a register the caller located, which is what makes this testable.
    pub fn using(port_mapper: CanonicalTool, hostname: impl Into<String>) -> Self {
        Self {
            port_mapper,
            hostname: hostname.into(),
        }
    }

    /// What this box holds, asking epmd only where epmd is already running.
    ///
    /// **A box with no port mapper is not a failure and not a question.** It is an
    /// installation with nothing up, reported from the process table alone, and the register
    /// is not touched: running it would start nothing, but running anything else afterwards
    /// would, and this is the one place that decides whether the rest of the facet asks at
    /// all.
    ///
    /// A resident port mapper that will not answer *is* a failure, because rastro cannot
    /// tell it apart from a box whose nodes it failed to find.
    pub fn read(&self, resident: &ResidentRuntime) -> Result<Installation, CollectionError> {
        if !resident.port_mapper_running() {
            return Ok(Installation::new(
                false,
                resident.broker_process_ids().len(),
                [],
            ));
        }

        let registered = EpmdRegister::parse(&self.port_mapper.run(&[NAMES])?)?;
        let nodes = registered
            .into_iter()
            .map(|node| {
                let name = NodeName::new(node.name, self.hostname.as_str())?;

                Ok((
                    name,
                    Node {
                        distribution_port: node.distribution_port,
                    },
                ))
            })
            .collect::<Result<Vec<_>, CollectionError>>()?;

        Ok(Installation::new(
            true,
            resident.broker_process_ids().len(),
            nodes,
        ))
    }
}
