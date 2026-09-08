//! Asking containerd what it is, through `ctr`.

use rastro_collector::{AbsolutePath, CollectionError};

use super::containerd_address::ContainerdAddress;
use super::ctr_version::{CtrClientVersion, CtrServerVersions};
use crate::collectors::canonical_tool::CanonicalTool;
use crate::collectors::containers::model::{
    ContainerdEngine, ContainerdNamespaces, ContainerdServer,
};
use crate::collectors::containers::value_objects::{EngineVersion, NamespaceName};

/// containerd's own client, which is the only interface it ships for asking these
/// questions.
const PROGRAM: &str = "ctr";

/// The flag naming the socket, which is passed before every subcommand because `ctr`'s own
/// default is wrong on any box that has docker.
const ADDRESS_FLAG: &str = "-a";

/// The client's own version, which needs no socket at all.
const CLIENT_VERSION: [&str; 1] = ["--version"];

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

    fn namespaces(&self, address: &AbsolutePath) -> Result<ContainerdNamespaces, CollectionError> {
        let listed = self.run(address, &["namespaces", "ls", "--quiet"])?;
        let mut namespaces = Vec::new();

        for line in listed.lines() {
            let name = line.trim();
            if name.is_empty() {
                continue;
            }

            namespaces.push(NamespaceName::new(name)?);
        }

        Ok(ContainerdNamespaces::new(namespaces))
    }

    /// One `ctr` run, with the address ahead of the subcommand where `ctr` wants it.
    fn run(&self, address: &AbsolutePath, arguments: &[&str]) -> Result<String, CollectionError> {
        let mut addressed = vec![ADDRESS_FLAG, address.as_str()];
        addressed.extend_from_slice(arguments);

        self.tool.run(&addressed)
    }
}
