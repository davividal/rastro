//! Asking podman what it is, without becoming it.

use std::path::Path;

use rastro_collector::{AbsolutePath, CollectionError, WalkedTree};

use super::podman_container_row::PodmanContainerRow;
use super::podman_info::PodmanInfoDocument;
use super::podman_layout::PodmanLayout;
use super::running_process::command_lines_of;
use crate::collectors::canonical_tool::CanonicalTool;
use crate::collectors::containers::model::{PodmanContainers, PodmanEngine};
use crate::collectors::containers::value_objects::EngineVersion;

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
    /// Where the store is, from podman's own configuration files rather than from podman.
    layout: PodmanLayout,
    /// The socket of a service that is already running, if one is.
    service: Option<AbsolutePath>,
}

impl Podman {
    /// Locates the client, reads the configuration, and looks for a running service.
    pub fn detect() -> Option<Self> {
        let tool = CanonicalTool::located(PROGRAM)?;

        Some(Self {
            tool,
            layout: PodmanLayout::discover(),
            service: serving_socket("/proc"),
        })
    }

    /// The same over a tool, a layout and a service the caller chose.
    pub fn using(tool: CanonicalTool, layout: PodmanLayout, service: Option<String>) -> Self {
        Self {
            tool,
            layout,
            service: service
                .and_then(|socket| AbsolutePath::new(socket, "podman service socket").ok()),
        }
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

/// The socket of a `podman system service` that is already running, if one is.
///
/// **The process, not the socket file.** A socket file exists whenever the unit is enabled,
/// which says nothing about whether anything is behind it, and connecting to a
/// socket-activated one *starts* the service — which then opens the store. Looking for the
/// process answers the real question and touches nothing.
///
/// The address comes from the service's own command line where it names one. Where systemd
/// passed it the socket instead, which is what socket activation does, the command line
/// names nothing and the documented default applies.
fn serving_socket(proc: impl AsRef<Path>) -> Option<AbsolutePath> {
    let serving = command_lines_of(proc, PROGRAM)
        .into_iter()
        .find(|arguments| {
            SERVICE_WORDS
                .iter()
                .all(|word| arguments.iter().any(|a| a == word))
        })?;

    let named = serving
        .iter()
        .find_map(|argument| argument.strip_prefix("unix://"))
        .map(str::to_owned)
        .unwrap_or_else(|| DEFAULT_SOCKET.to_owned());

    AbsolutePath::new(named, "podman service socket").ok()
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
