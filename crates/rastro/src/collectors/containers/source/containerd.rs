//! Asking containerd what it is, through `ctr`.

use rastro_collector::{AbsolutePath, CollectionError};

use super::containerd_address::ContainerdAddress;
use super::ctr_container_document::CtrContainerDocument;
use super::ctr_tasks::CtrTasks;
use super::ctr_version::{CtrClientVersion, CtrServerVersions};
use crate::collectors::canonical_tool::CanonicalTool;
use crate::collectors::containers::model::{
    ContainerdContainer, ContainerdEngine, ContainerdNamespace, ContainerdNamespaces,
    ContainerdServer, ContainerdTask, UnreadableObject,
};
use crate::collectors::containers::value_objects::{ContainerId, EngineVersion, NamespaceName};

/// containerd's own client, which is the only interface it ships for asking these
/// questions.
const PROGRAM: &str = "ctr";

/// The flag naming the socket, which is passed before every subcommand because `ctr`'s own
/// default is wrong on any box that has docker.
const ADDRESS_FLAG: &str = "-a";

/// The client's own version, which needs no socket at all.
const CLIENT_VERSION: [&str; 1] = ["--version"];

/// The flag naming the namespace, which every read below the engine needs.
const NAMESPACE_FLAG: &str = "-n";

/// The flag that reduces a list to identifiers alone.
const QUIET: &str = "--quiet";

/// The reason recorded for a box with `ctr` and nothing behind it.
const NOTHING_RUNNING: &str = "no containerd is running on this box, so there is no socket \
                               to ask: no containerd process was found and none of the \
                               addresses one publishes exists";

/// A containerd rastro can ask, and the address it will ask at.
///
/// **The client is what detection finds, the same as docker.** `ctr` ships with containerd,
/// so a box that has it has had containerd installed, and whether anything is answering is
/// then state rather than a question about whether the engine exists.
///
/// **`ctr` is a debug tool and says so**, which shapes every read here: `--quiet` wherever
/// it exists, so the answer is one identifier per line rather than an aligned table, and
/// `containers info` for the detail, which is JSON. Only `version` has neither, and it is
/// parsed with a failing arm rather than a forgiving one.
///
/// It passes the same gate docker does, measured on the same quiet box: `version`,
/// `namespaces ls` and `containers info` left a full stat inventory of `/var/lib/containerd`
/// and `/run/containerd` unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Containerd {
    tool: CanonicalTool,
    address: Option<AbsolutePath>,
}

impl Containerd {
    /// Locates the client and discovers where containerd is listening.
    pub fn detect() -> Option<Self> {
        let tool = CanonicalTool::located(PROGRAM)?;
        let address = ContainerdAddress::discover();

        Some(Self { tool, address })
    }

    /// The same source over a tool and an address the caller chose.
    ///
    /// The address is text rather than an [`AbsolutePath`] so a test can hand over what a
    /// host would report, including nothing at all.
    pub fn using(tool: CanonicalTool, address: Option<String>) -> Self {
        Self {
            tool,
            address: address
                .and_then(|address| AbsolutePath::new(address, "containerd address").ok()),
        }
    }

    /// containerd as this box has it: the client, and the engine if one answered.
    pub fn read(&self) -> Result<ContainerdEngine, CollectionError> {
        let client = self.client_version()?;

        let Some(address) = &self.address else {
            return Ok(ContainerdEngine::unreachable(client, NOTHING_RUNNING));
        };

        let server = CtrServerVersions::parse(&self.run(address, &["version"])?)?;

        Ok(ContainerdEngine::answering(
            client,
            ContainerdServer {
                version: server.version,
                revision: server.revision,
                address: address.clone(),
                namespaces: self.namespaces(address)?,
            },
        ))
    }

    /// The client's own version, which is readable whether or not anything is running.
    ///
    /// `ctr --version` never connects, which is the whole reason it is the read: measured on
    /// containerd 2.3.4, the `version` subcommand against an address with nothing behind it
    /// exits non-zero and prints nothing at all, the client's own half included.
    fn client_version(&self) -> Result<EngineVersion, CollectionError> {
        CtrClientVersion::parse(&self.tool.run(&CLIENT_VERSION)?)
    }

    /// Every namespace, with what is in each.
    fn namespaces(&self, address: &AbsolutePath) -> Result<ContainerdNamespaces, CollectionError> {
        let listed = self.run(address, &["namespaces", "ls", QUIET])?;
        let mut namespaces = Vec::new();

        for line in listed.lines() {
            let name = line.trim();
            if name.is_empty() {
                continue;
            }

            let name = NamespaceName::new(name)?;
            let held = self.held_by(address, &name)?;
            namespaces.push((name, held));
        }

        ContainerdNamespaces::new(namespaces)
    }

    /// The containers of one namespace, each with the task running it.
    ///
    /// **The tasks are read once for the namespace rather than once per container.**
    /// containerd offers no `tasks info`, so `tasks ls` is the only place a pid and a status
    /// exist, and it answers for every container in the namespace at once. A container with
    /// no row there is defined and not running, which is the state docker spells as a status
    /// on the container itself.
    fn held_by(
        &self,
        address: &AbsolutePath,
        namespace: &NamespaceName,
    ) -> Result<ContainerdNamespace, CollectionError> {
        let tasks = CtrTasks::parse(&self.in_namespace(address, namespace, &["tasks", "ls"])?)?;
        let listed = self.in_namespace(address, namespace, &["containers", "ls", QUIET])?;

        let mut read: Vec<(ContainerId, ContainerdContainer)> = Vec::new();
        let mut unreadable: Vec<UnreadableObject> = Vec::new();

        for line in listed.lines() {
            let id = line.trim();
            if id.is_empty() {
                continue;
            }

            let id = ContainerId::new(id)?;
            match self.inspect(address, namespace, &id, tasks.of(&id)) {
                Ok(container) => read.push(container),
                Err(failure) => {
                    unreadable.push(UnreadableObject::new(id.as_str(), &failure.to_string())?)
                }
            }
        }

        ContainerdNamespace::new(read, unreadable)
    }

    /// One container as containerd describes it.
    fn inspect(
        &self,
        address: &AbsolutePath,
        namespace: &NamespaceName,
        id: &ContainerId,
        task: Option<ContainerdTask>,
    ) -> Result<(ContainerId, ContainerdContainer), CollectionError> {
        let output = self.in_namespace(address, namespace, &["containers", "info", id.as_str()])?;
        let document: CtrContainerDocument = serde_json::from_str(&output).map_err(|error| {
            CollectionError::new(format!(
                "could not read what `{PROGRAM} containers info` reported as JSON: {error}"
            ))
        })?;

        document.to_container(task)
    }

    /// One `ctr` run inside a namespace.
    fn in_namespace(
        &self,
        address: &AbsolutePath,
        namespace: &NamespaceName,
        arguments: &[&str],
    ) -> Result<String, CollectionError> {
        let mut namespaced = vec![NAMESPACE_FLAG, namespace.as_str()];
        namespaced.extend_from_slice(arguments);

        self.run(address, &namespaced)
    }

    /// One `ctr` run, with the address ahead of the subcommand where `ctr` wants it.
    fn run(&self, address: &AbsolutePath, arguments: &[&str]) -> Result<String, CollectionError> {
        let mut addressed = vec![ADDRESS_FLAG, address.as_str()];
        addressed.extend_from_slice(arguments);

        self.tool.run(&addressed)
    }
}
