//! Composing the nodes to ask, from what is resident and what the register holds.
//!
//! The one place the facet's gate is enforced: the register is read only where the port
//! mapper is already running, because `epmd -names` is safe to run and a CLI tool is not,
//! and the whole point of asking epmd first is to avoid starting one.

use std::path::{Path, PathBuf};

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

/// Where the kernel publishes its process table.
const PROC: &str = "/proc";

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
    pub fn read(&self) -> Result<Installation, CollectionError> {
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
        let nodes = registered
            .into_iter()
            .map(|node| {
                let name = NodeName::new(node.name, hostname.as_str())?;

                Ok((
                    name,
                    Node {
                        distribution_port: node.distribution_port,
                    },
                ))
            })
            .collect::<Result<Vec<_>, CollectionError>>()?;

        Ok(Installation::new(true, brokers, nodes))
    }
}
