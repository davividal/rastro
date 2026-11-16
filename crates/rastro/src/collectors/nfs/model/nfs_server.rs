//! The kernel NFS server, as far as this run could read it.

use rastro_collector::Observation;

use super::export_table::ExportTable;
use super::module_parameters::ModuleParameters;
use super::server_runtime::ServerRuntime;

/// The server half of the facet.
///
/// **Three answers, because "no server" arrives by two routes and one of them is a guess.**
/// [`Self::NotHere`] is a box with no kernel server and nothing serving NFS from userspace.
/// [`Self::Unreadable`] is a server rastro knows of or cannot rule out, and does not read: one
/// whose filesystem it would have to mount, one in userspace, or a process table it could not
/// see all of. Reporting either as `null` would describe a file server as a box with none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerHalf {
    NotHere,
    Read(NfsServer),
    Unreadable(String),
}

/// What the kernel server exports and how it runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NfsServer {
    pub exports: ExportsReading,
    pub runtime: ServerRuntime,

    /// The `nfsd` module's parameters, `None` for a server built into the kernel with none.
    pub parameters: Option<ModuleParameters>,
}

/// The export table, or why it could not be read: it fails alone, and the runtime still reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportsReading {
    Read(ExportTable),
    Unreadable(String),
}

impl From<&ServerHalf> for Observation {
    fn from(half: &ServerHalf) -> Self {
        match half {
            ServerHalf::NotHere => Observation::null(),
            ServerHalf::Read(server) => Observation::from(server),
            ServerHalf::Unreadable(reason) => refusal(reason),
        }
    }
}

impl From<&NfsServer> for Observation {
    fn from(server: &NfsServer) -> Self {
        let exports = match &server.exports {
            ExportsReading::Read(table) => Observation::from(table),
            ExportsReading::Unreadable(reason) => refusal(reason),
        };

        Observation::object([
            ("exports", exports),
            (
                "parameters",
                server
                    .parameters
                    .as_ref()
                    .map_or_else(Observation::null, Observation::from),
            ),
            ("runtime", Observation::from(&server.runtime)),
        ])
    }
}

fn refusal(reason: &str) -> Observation {
    Observation::object([("error", Observation::text(reason))]).incomplete()
}
