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
use rastro::collectors::containers::{
    EngineInstance, EngineSource, Podman, PodmanLayout, accounts,
};
use rastro_collector::{ClaimedReading, Collector, Presence};
use rastro_fingerprint::{Observation, Volatility};
use support::fs_tree::{scratch_tree, write};
use support::observation::{boolean, field, integer, is_null, items_of, keys_of, text};

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

/// `podman --remote ps --all --format json`, verbatim from a service.
///
/// The two shapes that matter are both here: a running container whose `ExitedAt` is Go's
/// zero time in seconds, and a stopped one that really did exit. The bare-hex `ImageID` is
/// podman's spelling of what docker writes as `sha256:…`.
const CONTAINERS: &str = r#"[
  {
    "Id": "3f1c5e6a7b8c9d0e1f2a3b4c5d6e7f809a1b2c3d4e5f60718293a4b5c6d7e8f90",
    "Names": ["pweb"],
    "Image": "docker.io/library/alpine:latest",
    "ImageID": "1991bd789d7184290c3cce84fd6af068b8b745e9bddf178661ce7f5ecf68135c",
    "State": "running",
    "ExitCode": 0,
    "Created": 1789469574,
    "StartedAt": 1789469574,
    "ExitedAt": -62135596800,
    "Restarts": 0,
    "Pod": "",
    "IsInfra": false,
    "AutoRemove": false,
    "Labels": { "com.example.role": "web" },
    "Networks": ["podman"],
    "Ports": [
      {
        "host_ip": "127.0.0.1",
        "container_port": 80,
        "host_port": 18081,
        "range": 1,
        "protocol": "tcp"
      }
    ]
  },
  {
    "Id": "4e2d6f7a8b9c0d1e2f3a4b5c6d7e8f901a2b3c4d5e6f708192a3b4c5d6e7f801",
    "Names": ["pstopped"],
    "Image": "docker.io/library/alpine:latest",
    "ImageID": "1991bd789d7184290c3cce84fd6af068b8b745e9bddf178661ce7f5ecf68135c",
    "State": "exited",
    "ExitCode": 4,
    "Created": 1789469574,
    "StartedAt": 1789469574,
    "ExitedAt": 1789469580,
    "Restarts": 0,
    "Pod": "",
    "IsInfra": false,
    "AutoRemove": false,
    "Labels": null,
    "Networks": ["podman"],
    "Ports": null
  }
]"#;

/// A `podman` answering `--version` locally and everything else only in remote mode.
///
/// **The shim refuses a local subcommand, which is the point.** If the source ever called
/// `podman ps` without `--remote`, this fails rather than quietly answering: on a real box
/// that call would initialise the store, and no test should let it through unnoticed.
fn fake_podman(name: &str, info: &str) -> (CanonicalTool, std::path::PathBuf) {
    let containers = CONTAINERS;
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
ps)
if [ "$remote" = no ]; then
printf 'a local read would initialise the store: %s\n' "$*" >&2
exit 1
fi
cat <<'STDOUT'
{containers}
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

/// One engine's own entry, from the facet's `engines` half.
fn engine_of(facet: &Observation, flavour: &str) -> Observation {
    // Two keys, because a flavour holds instances: every user can run their own podman, so
    // the account that owns an engine is the second key. On these fixtures it is always
    // `root`, which is the ordinary box.
    field(&field(&field(facet, "engines"), flavour), "root")
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
    assert_eq!(
        keys_of(&field(&observed, "engines")),
        vec!["podman".to_owned()]
    );
    assert_eq!(
        keys_of(&field(&observed, "containers")),
        vec!["podman".to_owned()]
    );
}

#[test]
fn a_running_service_reports_itself_and_the_store_it_resolved() {
    // Arrange: the service's own account, which is a different fact from the configuration
    // rastro read to build the claim. A store moved in a file the service has not been
    // restarted to pick up is exactly the disagreement worth seeing.
    let podman = engine_of(
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
    let podman = engine_of(&podman_facet("no-service", INFO, None), "podman");

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

fn containers_of(facet: &Observation) -> Observation {
    field(&field(&field(facet, "containers"), "podman"), "root")
}

fn container_of(name: &str, container: &str) -> Observation {
    field(
        &containers_of(&podman_facet(name, INFO, Some(SOCKET.to_owned()))),
        container,
    )
}

#[test]
fn the_containers_are_keyed_by_name() {
    // Arrange: keyed the way docker's are, and for the same reason: a name outlives the id
    // it is minted with.
    let containers = containers_of(&podman_facet("names", INFO, Some(SOCKET.to_owned())));

    // Act & Assert
    assert_eq!(
        keys_of(&containers),
        vec!["pstopped".to_owned(), "pweb".to_owned()]
    );
}

#[test]
fn a_container_records_the_image_and_the_id_podman_resolved() {
    // Arrange: podman prints the image id as bare hex where docker writes `sha256:…`, and
    // each is recorded as its engine spells it rather than normalised into the other.
    let container = container_of("image", "pweb");

    // Act & Assert
    assert_eq!(
        text(&field(&container, "image")),
        "docker.io/library/alpine:latest"
    );
    assert_eq!(
        text(&field(&container, "image_id")),
        "1991bd789d7184290c3cce84fd6af068b8b745e9bddf178661ce7f5ecf68135c"
    );
    assert_eq!(text(&field(&container, "state")), "running");
}

#[test]
fn a_running_container_has_no_exit_stamp() {
    // Arrange: **the trap, in podman's spelling.** A running container reports
    // `"ExitedAt": -62135596800`, Go's zero time in whole seconds, which recorded as it
    // stands would read as a container that exited in the year one.
    let container = container_of("zero-time", "pweb");

    // Act & Assert
    assert!(is_null(&field(&container, "exited_seconds_since_epoch")));
    assert_eq!(
        integer(&field(&container, "created_seconds_since_epoch")),
        1_789_469_574
    );
}

#[test]
fn a_stopped_container_records_when_it_exited_and_with_what() {
    // Arrange
    let container = container_of("exited", "pstopped");

    // Act & Assert
    assert_eq!(integer(&field(&container, "exit_code")), 4);
    assert_eq!(
        integer(&field(&container, "exited_seconds_since_epoch")),
        1_789_469_580
    );
}

#[test]
fn the_stamps_are_volatile_and_the_state_is_not() {
    // Arrange: the same split docker's containers get. `running` becoming `exited` is the
    // line worth diffing; the seconds it happened at move on their own.
    let container = container_of("volatility", "pstopped");

    // Act & Assert
    assert_eq!(
        field(&container, "started_seconds_since_epoch").volatility(),
        Volatility::Volatile
    );
    assert_eq!(
        field(&container, "restarts").volatility(),
        Volatility::Volatile
    );
    assert_eq!(field(&container, "state").volatility(), Volatility::Stable);
}

#[test]
fn a_published_port_records_the_run_of_ports_it_covers() {
    // Arrange: **podman's own shape, not docker's.** `-p 8000-8010:8000-8010` is one
    // binding covering eleven ports here and eleven bindings in docker; flattening podman's
    // would mean inventing ten entries the engine never reported.
    let ports = field(&container_of("ports", "pweb"), "ports");
    let bindings = items_of(&field(&ports, "80/tcp"));

    // Act & Assert
    assert_eq!(keys_of(&ports), vec!["80/tcp".to_owned()]);
    assert_eq!(text(&field(&bindings[0], "host_address")), "127.0.0.1");
    assert_eq!(integer(&field(&bindings[0], "host_port")), 18081);
    assert_eq!(integer(&field(&bindings[0], "range")), 1);
}

#[test]
fn a_container_in_no_pod_records_none() {
    // Arrange: a pod is podman's own concept with no docker equivalent, and an empty string
    // is how podman says a container is standalone.
    let container = container_of("pod", "pweb");

    // Act & Assert
    assert!(is_null(&field(&container, "pod")));
    assert!(!boolean(&field(&container, "is_infra")));
}

#[test]
fn two_engines_of_one_flavour_are_keyed_by_the_accounts_that_own_them() {
    // Arrange: **the arrangement the instance level exists for.** root's podman and alice's
    // are separate engines with separate stores and separate sockets, and a container called
    // `web` in each is two different containers.
    let (root_tool, _) = fake_podman("two-root", INFO);
    let (alice_tool, _) = fake_podman("two-alice", INFO);
    let collector = ContainersCollector::reading(vec![
        EngineSource::Podman(Podman::using(
            root_tool,
            PodmanLayout::default(),
            Some(SOCKET.to_owned()),
        )),
        EngineSource::Podman(Podman::belonging_to(
            EngineInstance::new("alice").expect("a legal account name"),
            alice_tool,
            PodmanLayout::default(),
            Some("/run/user/1000/podman/podman.sock".to_owned()),
        )),
    ]);

    // Act
    let observed = collector.collect().expect("the fixtures are well formed");

    // Assert
    assert_eq!(
        keys_of(&field(&field(&observed, "engines"), "podman")),
        vec!["alice".to_owned(), "root".to_owned()]
    );
    assert_eq!(
        keys_of(&field(&field(&observed, "containers"), "podman")),
        vec!["alice".to_owned(), "root".to_owned()]
    );
    assert_eq!(
        text(&field(
            &field(&field(&field(&observed, "engines"), "podman"), "alice"),
            "client_version"
        )),
        "5.8.6"
    );
}

#[test]
fn an_account_is_read_from_passwd_by_the_uid_that_owns_the_service() {
    // Arrange: a rootless engine belongs to a user, and a document that called it `1000`
    // would make the reader go and look the number up. Read here rather than taken from the
    // `accounts` facet, because a collector may not read another collector.
    let root = scratch_tree("podman-passwd", &[]);
    write(
        &root,
        "etc/passwd",
        "root:x:0:0:root:/root:/bin/bash\n\
         alice:x:1000:1000:Alice:/home/alice:/bin/bash\n\
         broken-line-with-too-few-columns\n\
         bob:x:1001:1001:Bob:/home/bob:/usr/sbin/nologin\n",
    );

    // Act
    let known = accounts(&root);

    // Assert
    assert_eq!(known[&1000].name, "alice");
    assert_eq!(known[&1000].home, "/home/alice");
    assert_eq!(known[&0].home, "/root");
    assert_eq!(
        known.len(),
        3,
        "a line with the wrong number of columns is skipped rather than guessed at"
    );
}
