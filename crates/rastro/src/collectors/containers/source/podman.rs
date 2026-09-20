//! Asking podman what it is, without becoming it.

use std::path::Path;

use rastro_collector::{AbsolutePath, CollectionError, WalkedTree};

use super::accounts::{Account, accounts};
use super::podman_container_row::PodmanContainerRow;
use super::podman_info::PodmanInfoDocument;
use super::podman_layout::PodmanLayout;
use super::running_process::{RunningProcess, running};
use crate::collectors::canonical_tool::CanonicalTool;
use crate::collectors::containers::model::{PodmanContainers, PodmanEngine};
use crate::collectors::containers::value_objects::{EngineInstance, EngineVersion};

use std::collections::BTreeMap;

const PROGRAM: &str = "podman";

/// The client's own version, measured not to touch the store.
///
/// **The flag, never the subcommand.** `podman version` reports a *server* version, and in
/// local mode producing one means becoming the server: measured on a wiped box, the
/// subcommand created 22 entries and the flag created none.
const CLIENT_VERSION: [&str; 1] = ["--version"];

/// The arguments that make the same binary a client of a service rather than an engine.
const REMOTE: [&str; 1] = ["--remote"];
const URL_FLAG: &str = "--url";

/// The socket a root-owned service listens on when nothing says otherwise.
const DEFAULT_SOCKET: &str = "/run/podman/podman.sock";

/// The uid the system's own engines belong to.
const ROOT: u32 = 0;

/// The words in the command line of a process that is serving the API.
const SERVICE_WORDS: [&str; 2] = ["system", "service"];

/// The reason recorded for the ordinary podman box.
const NO_SERVICE: &str = "podman is installed and no `podman system service` is running, so \
                          there is nothing to ask: reading podman any other way would \
                          initialise its store, which would change the box being described";

/// podman as rastro is willing to read it.
///
/// **Two things about podman shape this whole source**, both measured and both in
/// `docs/decisions.md`. A local read *is* the engine — it opens the store, takes the locks
/// and probes the filesystem — so rastro never makes one beyond `--version`. And connecting
/// to a socket on spec is not safe either, because `podman.socket` is socket-activated and
/// connecting starts the service that then opens the store. So the service is found by its
/// *process*, which is a pure read, and only then is it asked.
///
/// What is left on a box with no service is the configuration, which is enough for the
/// filesystem claim and nothing else. That is reported as it is rather than as an absence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Podman {
    tool: CanonicalTool,
    /// Whose engine this is, which is what tells two podmans on one box apart.
    instance: EngineInstance,
    /// Where the store is, from podman's own configuration files rather than from podman.
    layout: PodmanLayout,
    /// The socket of a service that is already running, if one is.
    service: Option<AbsolutePath>,
}

impl Podman {
    /// Every podman on this box: the system's, and one per user running a service.
    ///
    /// **Several, because a box can have several.** podman is per-user by design, so root's
    /// engine and each rootless one are separate engines with separate stores, separate
    /// sockets and containers that may share names. Root's is reported whenever the binary
    /// is there, since its store and its claim exist whether or not a service is running; a
    /// user's is reported only when their service is, because without one there is nothing
    /// about them rastro can learn without changing the box.
    pub fn detect_all() -> Vec<Self> {
        let Some(tool) = CanonicalTool::located(PROGRAM) else {
            return Vec::new();
        };

        let known = accounts("/");
        let services = serving(running("/proc", PROGRAM));

        let mut engines = vec![Self {
            tool: tool.clone(),
            instance: EngineInstance::root(),
            layout: PodmanLayout::discover(),
            service: services.get(&ROOT).cloned(),
        }];

        for (user_id, socket) in services {
            if user_id == ROOT {
                continue;
            }

            engines.push(Self {
                tool: tool.clone(),
                instance: account_named(&known, user_id),
                layout: rootless_layout(known.get(&user_id), user_id),
                service: Some(socket),
            });
        }

        engines
    }

    /// The same over a tool, a layout and a service the caller chose, as root's.
    pub fn using(tool: CanonicalTool, layout: PodmanLayout, service: Option<String>) -> Self {
        Self::belonging_to(EngineInstance::root(), tool, layout, service)
    }

    /// The same, for an engine belonging to somebody else.
    pub fn belonging_to(
        instance: EngineInstance,
        tool: CanonicalTool,
        layout: PodmanLayout,
        service: Option<String>,
    ) -> Self {
        Self {
            tool,
            instance,
            layout,
            service: service
                .and_then(|socket| AbsolutePath::new(socket, "podman service socket").ok()),
        }
    }

    /// The account this podman belongs to.
    pub fn instance(&self) -> EngineInstance {
        self.instance.clone()
    }

    /// podman as this box has it: the client, and the service if one is running.
    pub fn read(&self) -> Result<PodmanEngine, CollectionError> {
        let client = EngineVersion::new(client_version(&self.tool.run(&CLIENT_VERSION)?)?)?;

        let Some(socket) = &self.service else {
            return Ok(PodmanEngine::unread(client, NO_SERVICE));
        };

        let reported = self.ask(socket, &["info", "--format", "json"])?;
        let document: PodmanInfoDocument = serde_json::from_str(&reported).map_err(|error| {
            CollectionError::new(format!(
                "could not read what `{PROGRAM} --remote info` reported as JSON: {error}"
            ))
        })?;

        Ok(PodmanEngine::answering(
            client,
            document.to_server(socket.clone(), self.containers(socket)?)?,
        ))
    }

    /// The containers the service holds, from one list rather than a read per container.
    ///
    /// **One call, unlike docker's.** podman's list carries what docker needs an `inspect`
    /// per container to say — the image, the state, the ports, the labels, the pod — so
    /// there is no id list to race against and no per-container loss to record.
    fn containers(&self, socket: &AbsolutePath) -> Result<PodmanContainers, CollectionError> {
        let listed = self.ask(socket, &["ps", "--all", "--format", "json"])?;
        let rows: Vec<PodmanContainerRow> = serde_json::from_str(&listed).map_err(|error| {
            CollectionError::new(format!(
                "could not read what `{PROGRAM} --remote ps` reported as JSON: {error}"
            ))
        })?;

        let mut containers = Vec::new();
        for row in &rows {
            containers.push(row.to_container()?);
        }

        PodmanContainers::new(containers)
    }

    /// The store and the runtime state, sealed, with the operator's volumes spared.
    ///
    /// **Made from the configuration rather than from podman**, which is what lets it exist
    /// on a box with no service: the claim is the one thing rastro can always say about
    /// podman, and it is the one that matters most for the walk. On the reference machine
    /// the store held 376,948 of the box's 834,466 entries.
    ///
    /// The volume tree is spared the way docker's is, except that podman's is *discovered*:
    /// `volume_path` is configurable and may sit outside the store entirely, in which case
    /// every child of the store is sealed and the volumes are left where the walk finds
    /// them.
    pub fn private_trees(&self) -> Vec<WalkedTree> {
        let mut trees = Vec::new();

        if let Some(tree) = self
            .layout
            .run_root
            .as_ref()
            .and_then(|root| WalkedTree::new(root.as_str()).ok())
        {
            trees.push(tree);
        }

        let Some(graph_root) = &self.layout.graph_root else {
            return trees;
        };

        let spared = self.layout.volume_path.as_ref().map(AbsolutePath::as_str);
        let Ok(children) = std::fs::read_dir(Path::new(graph_root.as_str())) else {
            return trees;
        };

        for child in children.flatten() {
            let path = child.path();
            if !path.is_dir() {
                continue;
            }

            let Some(path) = path.to_str() else {
                continue;
            };
            if spared == Some(path) {
                continue;
            }

            if let Ok(tree) = WalkedTree::new(path) {
                trees.push(tree);
            }
        }

        trees.sort();
        trees
    }

    /// One `--remote` run against the service's socket.
    fn ask(&self, socket: &AbsolutePath, arguments: &[&str]) -> Result<String, CollectionError> {
        let url = format!("unix://{}", socket.as_str());
        let mut remote = REMOTE.to_vec();
        remote.push(URL_FLAG);
        remote.push(&url);
        remote.extend_from_slice(arguments);

        self.tool.run(&remote)
    }
}

/// Every running `podman system service`, keyed by the account it belongs to.
///
/// **The process, not the socket file.** A socket file exists whenever the unit is enabled,
/// which says nothing about whether anything is behind it, and connecting to a
/// socket-activated one *starts* the service — which then opens the store. Looking for the
/// process answers the real question and touches nothing.
///
/// The address comes from the service's own command line where it names one. Where systemd
/// passed it the socket instead, which is what socket activation does, the command line
/// names nothing and the documented default for that account applies: `/run/podman` for
/// root, and the user's own runtime directory otherwise.
///
/// **One account running two services keeps the lower pid**, since `running` hands them
/// over in pid order and the first is kept. Arbitrary between two equally real services,
/// and the same arbitrary answer on the next run, which is what the document needs.
fn serving(processes: Vec<RunningProcess>) -> BTreeMap<u32, AbsolutePath> {
    let mut services = BTreeMap::new();

    for process in processes {
        let is_service = SERVICE_WORDS
            .iter()
            .all(|word| process.arguments.iter().any(|argument| argument == word));
        if !is_service {
            continue;
        }

        let named = process
            .arguments
            .iter()
            .find_map(|argument| argument.strip_prefix("unix://"))
            .map(str::to_owned)
            .unwrap_or_else(|| default_socket(process.user_id));

        if let Ok(socket) = AbsolutePath::new(named, "podman service socket") {
            services.entry(process.user_id).or_insert(socket);
        }
    }

    services
}

/// Where a service listens when nobody said: root's is system-wide, everyone else's is in
/// their own runtime directory.
fn default_socket(user_id: u32) -> String {
    match user_id {
        ROOT => DEFAULT_SOCKET.to_owned(),
        user_id => format!("/run/user/{user_id}/podman/podman.sock"),
    }
}

/// The name of an account, or its number where the box names it nothing.
///
/// A uid with no passwd entry is not an error: a service can run as a number nobody named,
/// and a document saying `1000` is still true where one saying nothing would not be.
fn account_named(known: &BTreeMap<u32, Account>, user_id: u32) -> EngineInstance {
    known
        .get(&user_id)
        .and_then(|account| EngineInstance::new(account.name.clone()).ok())
        .unwrap_or_else(|| {
            EngineInstance::new(user_id.to_string()).expect("a uid is a legal instance name")
        })
}

/// Where a rootless engine keeps its store.
///
/// Read from that user's own configuration, since podman's per-user files override the
/// system ones, and falling back to the per-user defaults: the store under their home, the
/// runtime state in their runtime directory.
fn rootless_layout(account: Option<&Account>, user_id: u32) -> PodmanLayout {
    match account {
        Some(account) => PodmanLayout::for_account(&account.home, user_id),
        None => PodmanLayout::default(),
    }
}

/// The version out of `podman --version`, whose line is `podman version 5.8.6`.
fn client_version(output: &str) -> Result<String, CollectionError> {
    output
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().last())
        .map(str::to_owned)
        .ok_or_else(|| {
            CollectionError::new(format!(
                "could not read what `{PROGRAM} --version` reported: no version in it"
            ))
        })
}
