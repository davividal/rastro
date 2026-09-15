//! The podman half of the `containers` facet.
//!
//! **Read through `podman --remote`, and only where a service is already running.** A local
//! podman read *is* the engine — it opens the store, takes the locks and probes the
//! filesystem — so rastro makes exactly one local call, `--version`, which is measured not to
//! touch anything. Everything else goes to a service somebody else chose to run.
//!
//! The fixtures are verbatim from podman: a service on 6.0.2 answering a client binary on
//! 5.8.6, which is why the entry carries both.

mod support;

use std::fs;
use std::os::unix::fs::PermissionsExt;

use rastro::collectors::ContainersCollector;
use rastro::collectors::canonical_tool::CanonicalTool;
use rastro::collectors::containers::{EngineSource, Podman, PodmanLayout};
use rastro_collector::{ClaimedReading, Collector, Presence};
use rastro_fingerprint::Observation;
use support::fs_tree::{scratch_tree, write};
use support::observation::{field, is_null, keys_of, text};

/// `podman --version`, the one local call rastro makes.
const CLIENT_VERSION: &str = "podman version 5.8.6";

/// `podman --remote info --format json`, trimmed to what the facet reads.
///
/// The `ociRuntime.version` blob and the package names around it are left in the engine's
/// answer and out of the document: they describe the distribution's packaging rather than
/// the box's state.
const INFO: &str = r#"{
  "host": {
    "ociRuntime": {
      "name": "crun",
      "package": "crun-1.28-1.fc44.aarch64",
      "path": "/usr/bin/crun"
    },
    "eventLogger": "journald",
    "cgroupVersion": "v2",
    "cgroupManager": "systemd",
    "databaseBackend": "sqlite"
  },
  "store": {
    "graphRoot": "/var/lib/containers/storage",
    "runRoot": "/run/containers/storage",
    "volumePath": "/var/lib/containers/storage/volumes",
    "graphDriverName": "overlay",
    "configFile": null
  },
  "version": {
    "APIVersion": "6.0.2",
    "Version": "6.0.2",
    "GoVersion": "go1.26.5",
    "OsArch": "linux/arm64"
  }
}"#;

const SOCKET: &str = "/run/podman/podman.sock";

/// A `podman` answering `--version` locally and everything else only in remote mode.
///
/// **The shim refuses a local subcommand, which is the point.** If the source ever called
/// `podman ps` without `--remote`, this fails rather than quietly answering: on a real box
/// that call would initialise the store, and no test should let it through unnoticed.
fn fake_podman(name: &str, info: &str) -> (CanonicalTool, std::path::PathBuf) {
    let root = scratch_tree(&format!("podman-{name}"), &[]);
    let directory = root.to_str().expect("a UTF-8 scratch path");
    let path = root.join("podman");
    fs::write(
        &path,
        format!(
            r#"#!/bin/sh
remote=no
for argument in "$@"; do
case "$argument" in
--version)
printf '%s\n' '{CLIENT_VERSION}'
exit 0
;;
--remote)
remote=yes
;;
info)
if [ "$remote" = no ]; then
printf 'a local read would initialise the store: %s\n' "$*" >&2
exit 1
fi
cat <<'STDOUT'
{info}
STDOUT
exit 0
;;
esac
done
printf 'unexpected invocation: %s\n' "$*" >&2
exit 1
"#
        ),
    )
    .expect("a writable script");
    let mut permissions = fs::metadata(&path).expect("metadata").permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&path, permissions).expect("an executable script");

    (
        CanonicalTool::located_in("podman", &[directory]).expect("the fake tool is locatable"),
        root,
    )
}

/// A store on disk, with the directories a real graph root holds.
fn store_at(name: &str, volumes_inside: bool) -> (std::path::PathBuf, PodmanLayout) {
    let root = scratch_tree(
        &format!("podman-store-{name}"),
        &[
            "storage/overlay",
            "storage/overlay-images",
            "storage/overlay-layers",
            "storage/overlay-containers",
            "storage/volumes",
            "run",
            "elsewhere",
        ],
    );
    let graph_root = root.join("storage");
    let volumes = match volumes_inside {
        true => graph_root.join("volumes"),
        false => root.join("elsewhere"),
    };

    write(
        &root,
        "etc/containers/storage.conf",
        &format!(
            "[storage]\ngraphroot = \"{}\"\nrunroot = \"{}\"\n",
            graph_root.to_str().expect("utf-8"),
            root.join("run").to_str().expect("utf-8")
        ),
    );
    write(
        &root,
        "etc/containers/containers.conf",
        &format!(
            "[engine]\nvolume_path = \"{}\"\n",
            volumes.to_str().expect("utf-8")
        ),
    );

    (root.clone(), PodmanLayout::under(&root))
}

fn podman_facet(name: &str, info: &str, service: Option<String>) -> Observation {
    let (tool, _) = fake_podman(name, info);
    let podman = Podman::using(tool, PodmanLayout::default(), service);

    ContainersCollector::reading(vec![EngineSource::Podman(podman)])
        .collect()
        .expect("the fixtures are well formed")
}

fn claimed(collector: &ContainersCollector) -> Vec<(String, ClaimedReading)> {
    let mut claims: Vec<(String, ClaimedReading)> = collector
        .filesystem_claims()
        .iter()
        .map(|claim| (claim.tree().as_str().to_owned(), claim.reading()))
        .collect();
    claims.sort_by(|left, right| left.0.cmp(&right.0));
    claims
}

#[test]
fn presence_is_present_when_podman_is_on_the_host() {
    // Arrange: installed is present, whether or not anything is running. That is the whole
    // point of the middle state for this dialect.
    let (tool, _) = fake_podman("present", INFO);
    let collector = ContainersCollector::reading(vec![EngineSource::Podman(Podman::using(
        tool,
        PodmanLayout::default(),
        None,
    ))]);

    // Act & Assert
    assert_eq!(collector.presence(), Presence::Present);
}

#[test]
fn the_facet_holds_podman_under_its_own_key() {
    // Act
    let observed = podman_facet("keyed", INFO, Some(SOCKET.to_owned()));

    // Assert
    assert_eq!(keys_of(&observed), vec!["podman".to_owned()]);
}

#[test]
fn a_running_service_reports_itself_and_the_store_it_resolved() {
    // Arrange: the service's own account, which is a different fact from the configuration
    // rastro read to build the claim. A store moved in a file the service has not been
    // restarted to pick up is exactly the disagreement worth seeing.
    let podman = field(
        &podman_facet("answering", INFO, Some(SOCKET.to_owned())),
        "podman",
    );
    let server = field(&podman, "server");
    let store = field(&server, "store");

    // Act & Assert
    assert_eq!(text(&field(&podman, "client_version")), "5.8.6");
    assert_eq!(text(&field(&podman, "service")), "answering");
    assert_eq!(text(&field(&server, "version")), "6.0.2");
    assert_eq!(text(&field(&server, "api_version")), "6.0.2");
    assert_eq!(text(&field(&server, "socket")), SOCKET);
    assert_eq!(text(&field(&server, "oci_runtime")), "crun");
    assert_eq!(text(&field(&server, "cgroup_manager")), "systemd");
    assert_eq!(text(&field(&server, "database_backend")), "sqlite");
    assert_eq!(text(&field(&store, "driver")), "overlay");
    assert_eq!(
        text(&field(&store, "graph_root")),
        "/var/lib/containers/storage"
    );
}

#[test]
fn a_box_with_no_service_is_installed_and_unread_with_the_reason() {
    // Arrange: **the ordinary podman box, not an error and not an absence.** podman is
    // daemonless, so a host can be full of running containers with no podman process at all.
    // Reading it any other way would initialise the store, so rastro says what it did not do
    // and why rather than leaving a reader to wonder whether it looked.
    let podman = field(&podman_facet("no-service", INFO, None), "podman");

    // Act & Assert
    assert_eq!(text(&field(&podman, "client_version")), "5.8.6");
    assert_eq!(text(&field(&podman, "service")), "unreachable");
    assert!(is_null(&field(&podman, "server")));
    let reason = text(&field(&podman, "service_reason"));
    assert!(reason.contains("initialise its store"), "got {reason:?}");
}

#[test]
fn the_store_is_sealed_child_by_child_with_the_volume_tree_spared() {
    // Arrange: the claim is built from the configuration rather than from podman, which is
    // what lets it exist on a box with no service — and it is the one thing rastro can
    // always say about podman. On the reference machine the store held 376,948 of the box's
    // 834,466 entries.
    let (root, layout) = store_at("sealed", true);
    let (tool, _) = fake_podman("sealed-tool", INFO);
    let collector = ContainersCollector::reading(vec![EngineSource::Podman(Podman::using(
        tool, layout, None,
    ))]);
    let storage = root.join("storage");
    let storage = storage.to_str().expect("utf-8");

    // Act
    let claims = claimed(&collector);
    let trees: Vec<String> = claims.iter().map(|(tree, _)| tree.clone()).collect();

    // Assert
    assert!(trees.contains(&format!("{storage}/overlay")));
    assert!(trees.contains(&format!("{storage}/overlay-images")));
    assert!(trees.contains(&format!("{storage}/overlay-containers")));
    assert!(
        !trees.contains(&format!("{storage}/volumes")),
        "the operator's volumes must never be sealed: {trees:?}"
    );
    assert!(
        trees.contains(&root.join("run").to_str().expect("utf-8").to_owned()),
        "the run root is sealed whole, since nothing under it is the operator's: {trees:?}"
    );
    assert!(
        claims
            .iter()
            .all(|(_, reading)| *reading == ClaimedReading::Sealed)
    );
}

#[test]
fn a_volume_tree_moved_out_of_the_store_leaves_every_child_sealed() {
    // Arrange: podman's `volume_path` is configurable and may sit outside the store
    // entirely, unlike docker's. Then there is nothing under the store to spare, and the
    // volumes are left where the walk finds them rather than claimed.
    let (root, layout) = store_at("moved", false);
    let (tool, _) = fake_podman("moved-tool", INFO);
    let collector = ContainersCollector::reading(vec![EngineSource::Podman(Podman::using(
        tool, layout, None,
    ))]);
    let storage = root.join("storage");
    let storage = storage.to_str().expect("utf-8");

    // Act
    let trees: Vec<String> = claimed(&collector)
        .iter()
        .map(|(tree, _)| tree.clone())
        .collect();

    // Assert
    assert!(trees.contains(&format!("{storage}/volumes")));
    assert!(
        !trees.contains(&root.join("elsewhere").to_str().expect("utf-8").to_owned()),
        "the volume tree itself is never claimed, wherever it is: {trees:?}"
    );
}
