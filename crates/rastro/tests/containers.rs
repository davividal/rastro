//! The `containers` facet: which container engines are on the box, and what each is running.
//!
//! The fixtures are real output, captured from docker 29.8.0 with containerd 2.3.4 under it,
//! and trimmed to the fields this facet reads plus a few it deliberately ignores. Trimmed
//! rather than invented: a fixture written from memory tests rastro against the author's
//! recollection of docker, which is the mistake the nginx grammar entry in
//! `docs/decisions.md` records paying for.

mod support;

use std::fs;
use std::os::unix::fs::PermissionsExt;

use rastro::collectors::ContainersCollector;
use rastro::collectors::canonical_tool::CanonicalTool;
use rastro::collectors::containers::{Docker, EngineSource};
use rastro_collector::{Collector, Presence};
use rastro_fingerprint::Observation;
use support::fs_tree::scratch_tree;
use support::observation::{boolean, field, is_null, items_of, keys_of, text};

/// `docker version --format '{{json .}}'` on a box whose daemon answers.
///
/// The components are the reason this probe is worth reading rather than only being a
/// liveness check: they say which containerd and which runc the engine actually runs, and a
/// runc upgrade is exactly the change a fingerprint is taken to catch.
const VERSION_ANSWERING: &str = r#"{
  "Client": { "Version": "29.8.0", "ApiVersion": "1.56", "Context": "default" },
  "Server": {
    "Platform": { "Name": "Docker Engine - Community" },
    "Version": "29.8.0",
    "ApiVersion": "1.56",
    "MinAPIVersion": "1.40",
    "Components": [
      { "Name": "Engine", "Version": "29.8.0", "Details": { "GitCommit": "3ce5872" } },
      { "Name": "containerd", "Version": "v2.3.4", "Details": { "GitCommit": "db88095" } },
      { "Name": "runc", "Version": "1.5.1", "Details": { "GitCommit": "v1.5.1-0-g8f2685a" } },
      { "Name": "docker-init", "Version": "0.19.0", "Details": { "GitCommit": "de40ad0" } }
    ],
    "KernelVersion": "7.1.3-200.fc44.aarch64"
  }
}"#;

/// The same probe on a box where docker is installed and its daemon is not answering.
///
/// **Measured on docker 29.8.0, because it decides the whole detection ladder.** `docker
/// version` exits *zero* here, prints the client half on stdout with `"Server": null`, and
/// writes the connection failure to stderr. `docker info` and `docker ps` exit 1 in the same
/// situation, which is why this is the probe and neither of those is.
///
/// Docker 26.1.5 exits 1 for the same read, so this state is only reachable on a newer
/// client; the older one produces a facet `error` instead. Both are recorded in
/// `docs/decisions.md`, and the shape is what podman and containerd will report through.
const VERSION_UNREACHABLE: &str = r#"{
  "Client": { "Version": "29.8.0", "ApiVersion": "1.56", "Context": "default" },
  "Server": null
}"#;

const UNREACHABLE_STDERR: &str = "failed to connect to the docker API at \
unix:///var/run/docker.sock; check if the path is correct and if the daemon is running: \
dial unix /var/run/docker.sock: connect: no such file or directory";

/// `docker info --format '{{json .}}'`, trimmed.
///
/// `Containers`, `Images` and `NCPU` are kept deliberately: they are counts this facet does
/// not read, because a count says nothing about which container changed and the CPU count is
/// the host's business, and a fixture that dropped them could not catch a reader that started
/// using them.
const INFO_ANSWERING: &str = r#"{
  "ID": "d94030bd-2938-4d0a-9d0e-000000000000",
  "Containers": 1,
  "Images": 2,
  "NCPU": 7,
  "Driver": "overlayfs",
  "DockerRootDir": "/var/lib/docker",
  "CgroupDriver": "cgroupfs",
  "CgroupVersion": "2",
  "LoggingDriver": "json-file",
  "DefaultRuntime": "runc",
  "LiveRestoreEnabled": false,
  "SecurityOptions": ["name=seccomp,profile=builtin", "name=cgroupns"],
  "Swarm": { "NodeID": "", "LocalNodeState": "inactive", "ControlAvailable": false },
  "ServerVersion": "29.8.0"
}"#;

/// A `docker` that answers the two probes from fixtures, and refuses anything else loudly.
///
/// Refusing the unexpected is what makes the shim a test rather than a mock that agrees with
/// whatever it is asked: a source that started calling a subcommand nobody wrote a fixture
/// for fails here instead of quietly reading an empty answer.
fn fake_docker(name: &str, version: &str, version_stderr: &str, info: &str) -> Docker {
    let root = scratch_tree(&format!("containers-{name}"), &[]);
    let directory = root.to_str().expect("a UTF-8 scratch path");
    let path = root.join("docker");
    fs::write(
        &path,
        format!(
            r#"#!/bin/sh
case "$1" in
version)
cat <<'STDOUT'
{version}
STDOUT
printf '%s' '{version_stderr}' >&2
;;
info)
cat <<'STDOUT'
{info}
STDOUT
;;
*)
printf 'unexpected invocation: %s\n' "$*" >&2
exit 1
;;
esac
"#
        ),
    )
    .expect("a writable script");
    let mut permissions = fs::metadata(&path).expect("metadata").permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&path, permissions).expect("an executable script");

    Docker::using(
        CanonicalTool::located_in("docker", &[directory]).expect("the fake tool is locatable"),
    )
}

fn docker_facet(name: &str, version: &str, version_stderr: &str, info: &str) -> Observation {
    ContainersCollector::reading(vec![EngineSource::Docker(fake_docker(
        name,
        version,
        version_stderr,
        info,
    ))])
    .collect()
    .expect("the fixtures are well formed")
}

fn answering_docker(name: &str) -> Observation {
    field(
        &docker_facet(name, VERSION_ANSWERING, "", INFO_ANSWERING),
        "docker",
    )
}

fn answering_server(name: &str) -> Observation {
    field(&answering_docker(name), "server")
}

#[test]
fn presence_is_absent_when_no_container_engine_is_on_the_host() {
    // Arrange
    let collector = ContainersCollector::reading(Vec::new());

    // Act & Assert
    assert_eq!(collector.presence(), Presence::Absent);
}

#[test]
fn presence_is_present_when_docker_is_on_the_host() {
    // Arrange
    let collector = ContainersCollector::reading(vec![EngineSource::Docker(fake_docker(
        "present",
        VERSION_ANSWERING,
        "",
        INFO_ANSWERING,
    ))]);

    // Act & Assert
    assert_eq!(collector.presence(), Presence::Present);
}

#[test]
fn the_facet_is_keyed_by_the_engine() {
    // Act
    let observed = docker_facet("keyed", VERSION_ANSWERING, "", INFO_ANSWERING);

    // Assert
    assert_eq!(keys_of(&observed), vec!["docker".to_owned()]);
}

#[test]
fn an_answering_docker_reports_what_its_daemon_is_running_with() {
    // Act
    let docker = answering_docker("answering");
    let server = field(&docker, "server");

    // Assert
    assert_eq!(text(&field(&docker, "client_version")), "29.8.0");
    assert_eq!(text(&field(&docker, "daemon")), "answering");
    assert_eq!(text(&field(&server, "version")), "29.8.0");
    assert_eq!(text(&field(&server, "root_directory")), "/var/lib/docker");
    assert_eq!(text(&field(&server, "storage_driver")), "overlayfs");
    assert_eq!(text(&field(&server, "logging_driver")), "json-file");
    assert_eq!(text(&field(&server, "default_runtime")), "runc");
    assert_eq!(text(&field(&server, "swarm")), "inactive");
    assert!(!boolean(&field(&server, "live_restore")));
}

#[test]
fn a_cgroup_driver_and_version_are_reported_as_the_daemon_resolved_them() {
    // Arrange: the pair decides whether a container's memory and pids limits can apply at
    // all, so a v1 box and a v2 box are different state even with identical containers.
    let cgroup = field(&answering_server("cgroup"), "cgroup");

    // Act & Assert
    assert_eq!(text(&field(&cgroup, "driver")), "cgroupfs");
    assert_eq!(text(&field(&cgroup, "version")), "2");
}

#[test]
fn the_security_options_are_sorted_rather_than_left_in_the_daemons_order() {
    // Arrange: docker promises nothing about the order of this list, and an order that moved
    // between two runs of an unchanged box would break byte-identity.
    let options = items_of(&field(&answering_server("security"), "security_options"));

    // Act & Assert
    assert_eq!(
        options.iter().map(text).collect::<Vec<String>>(),
        vec![
            "name=cgroupns".to_owned(),
            "name=seccomp,profile=builtin".to_owned()
        ]
    );
}

#[test]
fn the_components_report_which_containerd_and_runc_the_engine_runs() {
    // Arrange
    let components = field(&answering_server("components"), "components");

    // Act & Assert
    assert_eq!(text(&field(&components, "containerd")), "v2.3.4");
    assert_eq!(text(&field(&components, "runc")), "1.5.1");
}

#[test]
fn a_docker_whose_daemon_does_not_answer_is_installed_and_unreachable() {
    // Arrange: the box has docker and no running daemon, which is state rather than a
    // failure to read, and is a different fact from having no docker at all.
    let observed = docker_facet("unreachable", VERSION_UNREACHABLE, UNREACHABLE_STDERR, "");

    // Act
    let docker = field(&observed, "docker");

    // Assert
    assert_eq!(text(&field(&docker, "client_version")), "29.8.0");
    assert_eq!(text(&field(&docker, "daemon")), "unreachable");
    assert!(text(&field(&docker, "daemon_reason")).contains("docker.sock"));
    assert!(is_null(&field(&docker, "server")));
}

#[test]
fn an_answering_daemon_records_no_reason_to_be_unreachable() {
    // Arrange
    let docker = answering_docker("no-reason");

    // Act & Assert
    assert!(is_null(&field(&docker, "daemon_reason")));
}

#[test]
fn output_that_is_not_json_fails_the_facet_rather_than_reading_as_an_empty_engine() {
    // Arrange
    let collector = ContainersCollector::reading(vec![EngineSource::Docker(fake_docker(
        "garbage",
        "not json at all",
        "",
        INFO_ANSWERING,
    ))]);

    // Act
    let failure = collector.collect().expect_err("garbage is not an engine");

    // Assert
    assert!(
        failure.to_string().contains("docker version"),
        "the failure should name the probe that produced it: {failure}"
    );
}
