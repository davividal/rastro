//! Where podman keeps what it holds, read from its configuration rather than from podman.

use std::fs;
use std::path::Path;

use serde::Deserialize;

use rastro_collector::AbsolutePath;

/// The three places podman documents as its defaults for a root-owned store.
const DEFAULT_GRAPH_ROOT: &str = "/var/lib/containers/storage";
const DEFAULT_RUN_ROOT: &str = "/run/containers/storage";
/// The volume tree's default is relative to the graph root rather than absolute.
const DEFAULT_VOLUMES: &str = "volumes";

/// Where the store is configured, distributed defaults first and the operator's second.
const STORAGE_FILES: [&str; 2] = [
    "usr/share/containers/storage.conf",
    "etc/containers/storage.conf",
];

/// Where a user's own overrides live, relative to their home.
const USER_STORAGE_FILE: &str = ".config/containers/storage.conf";
const USER_ENGINE_FILE: &str = ".config/containers/containers.conf";

/// Where a user's store and runtime state are when they have said nothing.
const USER_GRAPH_ROOT: &str = ".local/share/containers/storage";

/// Where the volume tree is configured, which is a different file from the store's.
const ENGINE_FILES: [&str; 2] = [
    "usr/share/containers/containers.conf",
    "etc/containers/containers.conf",
];

/// Where podman keeps its store, its runtime state and the operator's volumes.
///
/// **Read from configuration files, because asking podman would change the box.** Every
/// other engine in this facet is asked where it keeps things; podman cannot be, since a
/// local `podman info` initialises the store it would be describing — the measurement is in
/// `docs/decisions.md`. Its configuration is ordinary TOML in documented places, so the same
/// answer is available without running anything.
///
/// **This is the only thing rastro can learn about podman on a box with no service.** It is
/// enough for the claim, which is what stops the walk descending into a store that is mostly
/// image layers, and it is why the claim does not depend on the dialect being readable.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PodmanLayout {
    pub graph_root: Option<AbsolutePath>,
    pub run_root: Option<AbsolutePath>,
    /// The operator's own data, which the claim must never seal.
    pub volume_path: Option<AbsolutePath>,
}

impl PodmanLayout {
    /// The layout on this host.
    pub fn discover() -> Self {
        Self::under("/")
    }

    /// The layout of one user's own podman.
    ///
    /// **A rootless engine is configured somewhere else and defaults somewhere else**, which
    /// is most of why it needs its own reading: the store is under the user's home rather
    /// than in `/var/lib`, the runtime state is in their runtime directory rather than in
    /// `/run`, and their `~/.config/containers` overrides the system files rather than
    /// being overridden by them.
    pub fn for_account(home: &str, user_id: u32) -> Self {
        let stored = read_files::<StorageConfiguration>(Path::new(home), &[USER_STORAGE_FILE]);
        let engine = read_files::<EngineConfiguration>(Path::new(home), &[USER_ENGINE_FILE]);

        let graph_root = stored
            .iter()
            .filter_map(|file| file.storage.as_ref()?.graphroot.clone())
            .next_back()
            .unwrap_or_else(|| format!("{home}/{USER_GRAPH_ROOT}"));

        let run_root = stored
            .iter()
            .filter_map(|file| file.storage.as_ref()?.runroot.clone())
            .next_back()
            .unwrap_or_else(|| format!("/run/user/{user_id}/containers"));

        let volume_path = engine
            .iter()
            .filter_map(|file| file.engine.as_ref()?.volume_path.clone())
            .next_back()
            .unwrap_or_else(|| format!("{graph_root}/{DEFAULT_VOLUMES}"));

        Self {
            graph_root: AbsolutePath::new(graph_root, "podman graph root").ok(),
            run_root: AbsolutePath::new(run_root, "podman run root").ok(),
            volume_path: AbsolutePath::new(volume_path, "podman volume path").ok(),
        }
    }

    /// The same under a filesystem root the caller chose, which is what the tests hand it.
    pub fn under(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref();
        let stored = read_files::<StorageConfiguration>(root, &STORAGE_FILES);
        let engine = read_files::<EngineConfiguration>(root, &ENGINE_FILES);

        let graph_root = stored
            .iter()
            .filter_map(|file| file.storage.as_ref()?.graphroot.clone())
            .next_back()
            .unwrap_or_else(|| DEFAULT_GRAPH_ROOT.to_owned());

        let run_root = stored
            .iter()
            .filter_map(|file| file.storage.as_ref()?.runroot.clone())
            .next_back()
            .unwrap_or_else(|| DEFAULT_RUN_ROOT.to_owned());

        let volume_path = engine
            .iter()
            .filter_map(|file| file.engine.as_ref()?.volume_path.clone())
            .next_back()
            .unwrap_or_else(|| format!("{graph_root}/{DEFAULT_VOLUMES}"));

        Self {
            graph_root: AbsolutePath::new(graph_root, "podman graph root").ok(),
            run_root: AbsolutePath::new(run_root, "podman run root").ok(),
            volume_path: AbsolutePath::new(volume_path, "podman volume path").ok(),
        }
    }
}

/// The files that exist, parsed, in the order podman reads them.
///
/// A file that will not parse is skipped rather than failing the read: a half-written config
/// is a box somebody is in the middle of changing, and podman will fall back to its own
/// defaults too. A claim against the documented default is better than one against a path
/// invented from half a file.
fn read_files<T: serde::de::DeserializeOwned>(root: &Path, files: &[&str]) -> Vec<T> {
    files
        .iter()
        .filter_map(|file| fs::read_to_string(root.join(file)).ok())
        .filter_map(|text| toml::from_str::<T>(&text).ok())
        .collect()
}

/// The two lines of `storage.conf` this read is about.
#[derive(Debug, Default, Deserialize)]
struct StorageConfiguration {
    storage: Option<StorageSection>,
}

#[derive(Debug, Deserialize)]
struct StorageSection {
    graphroot: Option<String>,
    runroot: Option<String>,
}

/// The one line of `containers.conf` this read is about.
#[derive(Debug, Default, Deserialize)]
struct EngineConfiguration {
    engine: Option<EngineSection>,
}

#[derive(Debug, Deserialize)]
struct EngineSection {
    volume_path: Option<String>,
}
