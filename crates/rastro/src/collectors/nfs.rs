//! The kernel NFS server and client: what the server exports and how both halves run.
//!
//! **Server half.** Dispatched from the `nfsd` filesystem, which Debian mounts when the server
//! starts. The exports come from etab and the runtime from that filesystem, because both are
//! what the running server uses; `/etc/exports` and `/etc/nfs.conf` are only what it was asked
//! for, and the walk already sees them. nfs-ganesha is refused by name, since it serves NFS
//! with none of the kernel's interfaces and would otherwise read as no server.
//!
//! **Client half.** The mounts are the `mounts` facet's, with the options the kernel actually
//! mounted them with. What is left is the client modules' parameters, which nothing else reads.
//!
//! Three layers, and the dependency arrows only point one way:
//! [`source`] knows [`model`], `model` knows [`value_objects`], and neither of the last two
//! knows a host interface exists.

pub mod model;
pub mod source;
pub mod value_objects;

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

pub use model::{
    ClientExport, ExportTable, ExportsReading, ModuleParameters, NfsClient, NfsReport, NfsServer,
    SecurityGroup, ServerHalf, ServerRuntime,
};
pub use source::{Etab, ModuleParameterDirectory, NfsdFilesystem, UserspaceServer};
pub use value_objects::{ExportClient, ExportOption, KernelValue, ParameterName, SecurityFlavour};

// One import, because `rastro-collector` re-exports what an author needs. A
// collector written outside this repo looks exactly like this.
use rastro_collector::{
    CollectionError, Collector, CollectorCategory, CollectorId, CollectorIdentity,
    CollectorVersion, FacetName, FilesystemClaim, Observation, Presence, WalkedTree,
};

use source::userspace_server;

/// The kernel server's module.
const SERVER_MODULE: &str = "nfsd";

/// The kernel client's modules: `nfs` for every mount, `nfsv4` loaded on the first v4 one.
const CLIENT_MODULES: [&str; 2] = ["nfs", "nfsv4"];

/// The state nfs-utils keeps that moves with clients rather than with configuration, each
/// measured on Debian 12: the v4 client-tracking stores change on a client's first connect, and
/// statd keeps a file per peer it watches.
const SEALED_STATE: [&str; 5] = ["nfsdcld", "nfsdcltrack", "sm", "sm.bak", "v4recovery"];

/// mountd's record of v3 mounts, rewritten as clients mount and unmount.
const CHURNING_STATE: [&str; 1] = ["rmtab"];

/// Where each interface the facet reads is, so a test can put them elsewhere.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NfsHost {
    pub proc: PathBuf,
    pub sys_module: PathBuf,
    pub nfsd: PathBuf,
    pub state_directory: PathBuf,
}

impl NfsHost {
    pub fn this_host() -> Self {
        Self {
            proc: PathBuf::from("/proc"),
            sys_module: PathBuf::from("/sys/module"),
            nfsd: NfsdFilesystem::new().root().to_path_buf(),
            state_directory: Etab::new().state_directory().to_path_buf(),
        }
    }
}

pub struct NfsCollector {
    name: FacetName,
    identity: CollectorIdentity,
    host: NfsHost,
}

impl NfsCollector {
    pub fn new() -> Self {
        Self::reading(NfsHost::this_host())
    }

    pub fn reading(host: NfsHost) -> Self {
        Self {
            name: FacetName::new("nfs").expect("`nfs` is a legal facet name"),
            identity: CollectorIdentity::new(
                CollectorId::new("nfs").expect("`nfs` is a legal collector id"),
                CollectorVersion::new("1").expect("`1` is a legal collector version"),
            ),
            host,
        }
    }

    fn modules(&self) -> ModuleParameterDirectory {
        ModuleParameterDirectory::at(&self.host.sys_module)
    }

    fn nfsd(&self) -> NfsdFilesystem {
        NfsdFilesystem::at(&self.host.nfsd)
    }

    /// Ganesha first: it and the kernel server cannot share a port, and a ganesha box may still
    /// have `nfsd` loaded from an earlier start.
    fn server(&self, userspace: &UserspaceServer) -> ServerHalf {
        if let UserspaceServer::Running { process_id } = userspace {
            return ServerHalf::Unreadable(format!(
                "nfs-ganesha (pid {process_id}) is serving NFS, and rastro reads only the kernel server"
            ));
        }

        let nfsd = self.nfsd();
        if nfsd.is_mounted() {
            let exports = match Etab::in_directory(&self.host.state_directory).read() {
                Ok(table) => ExportsReading::Read(table),
                Err(error) => ExportsReading::Unreadable(error.to_string()),
            };
            return ServerHalf::Read(NfsServer {
                exports,
                runtime: nfsd.runtime(),
                parameters: self.modules().parameters_of(SERVER_MODULE),
            });
        }

        if self.modules().is_loaded(SERVER_MODULE) {
            return ServerHalf::Unreadable(format!(
                "the nfsd module is loaded and its filesystem is not mounted at {}, which rastro \
                 does not do for it",
                nfsd.root().display()
            ));
        }

        match userspace {
            UserspaceServer::Unseen(reason) => ServerHalf::Unreadable(format!(
                "{reason}, so a server in userspace cannot be ruled out"
            )),
            _ => ServerHalf::NotHere,
        }
    }

    fn client(&self) -> Option<NfsClient> {
        let modules = self.modules();
        if !modules.is_loaded(CLIENT_MODULES[0]) {
            return None;
        }

        Some(NfsClient {
            parameters: CLIENT_MODULES
                .iter()
                .map(|module| (*module, modules.parameters_of(module)))
                .collect::<BTreeMap<_, _>>(),
        })
    }
}

impl Default for NfsCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl Collector for NfsCollector {
    fn name(&self) -> &FacetName {
        &self.name
    }

    fn identity(&self) -> &CollectorIdentity {
        &self.identity
    }

    fn category(&self) -> CollectorCategory {
        CollectorCategory::State
    }

    /// `undetermined` where the module directory is missing, since then a loaded server and no
    /// server look the same, and where processes went unseen with nothing else to go on.
    fn presence(&self) -> Presence {
        if !self.host.sys_module.is_dir() {
            return Presence::Undetermined {
                reason: format!(
                    "{} is missing, so which NFS modules are loaded cannot be told",
                    self.host.sys_module.display()
                ),
            };
        }

        let modules = self.modules();
        let kernel_side = self.nfsd().is_mounted()
            || modules.is_loaded(SERVER_MODULE)
            || modules.is_loaded(CLIENT_MODULES[0]);
        if kernel_side {
            return Presence::Present;
        }

        match userspace_server(&self.host.proc) {
            UserspaceServer::Running { .. } => Presence::Present,
            UserspaceServer::NotRunning => Presence::Absent,
            UserspaceServer::Unseen(reason) => Presence::Undetermined {
                reason: format!("{reason}, so a server in userspace cannot be ruled out"),
            },
        }
    }

    fn collect(&self) -> Result<Observation, CollectionError> {
        let userspace = userspace_server(&self.host.proc);

        Ok(Observation::from(&NfsReport {
            client: self.client(),
            server: self.server(&userspace),
        }))
    }

    /// Only what is there: a claim the host does not resolve is left unmade.
    fn filesystem_claims(&self) -> Vec<FilesystemClaim> {
        let sealed = SEALED_STATE
            .iter()
            .filter_map(|name| self.present_state(name).map(FilesystemClaim::sealed));
        let churning = CHURNING_STATE
            .iter()
            .filter_map(|name| self.present_state(name).map(FilesystemClaim::churns));

        sealed.chain(churning).collect()
    }
}

impl NfsCollector {
    /// The tree an entry of nfs-utils' state directory is, where the entry exists.
    fn present_state(&self, name: &str) -> Option<WalkedTree> {
        let path = self.host.state_directory.join(name);
        fs::symlink_metadata(&path).ok()?;
        WalkedTree::new(path.to_str()?).ok()
    }
}
