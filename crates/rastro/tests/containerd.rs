//! The containerd half of the `containers` facet.
//!
//! Its own file, because the two dialects share only their value objects: containerd
//! describes a container one level below docker, and the fixtures are `ctr`'s output rather
//! than an API's JSON.
//!
//! The fixtures are verbatim from containerd 2.3.4 as docker 29.8.0 ships it.

mod support;

use std::fs;
use std::os::unix::fs::PermissionsExt;

use rastro::collectors::ContainersCollector;
use rastro::collectors::canonical_tool::CanonicalTool;
use rastro::collectors::containers::{Containerd, EngineSource};
use rastro_collector::{ClaimedReading, Collector, Presence};
use rastro_fingerprint::{Observation, Volatility};
use support::fs_tree::scratch_tree;
use support::observation::{field, integer, is_null, items_of, keys_of, text};

/// `ctr version`, which is text and not JSON: `ctr` offers no machine-readable form of it.
///
/// Two blocks with the same keys in each, which is the shape the parse is written to, and
/// the reason a client and a server version can be told apart at all.
const VERSION: &str = "Client:
  Version:  v2.3.4
  Revision: db8809540e1a7a9da5d518876894933ff55692ab
  Go version: go1.26.8

Server:
  Version:  v2.3.4
  Revision: db8809540e1a7a9da5d518876894933ff55692ab
  UUID: 0860d515-9359-424f-922a-c1a59d58096b
";

/// `ctr -n moby containers info <id>`, with containerd's whole OCI spec removed.
///
/// The spec is thirty kilobytes of the thirty-three this command prints, and none of it is
/// read: what a container is allowed to do is docker's account to give on a docker box, and
/// the deserializer does not declare the field.
///
/// `SnapshotKey` and `Snapshotter` are empty here, and that is not an oversight in the
/// fixture: docker keeps its own snapshots and hands containerd a prepared rootfs, so a
/// container docker created has neither. One created through containerd itself does.
const CONTAINER_INFO: &str = r#"{
    "ID": "bf4ea5bdd32301e4a7f81b39ea157d37e0b992306c6605fa2f52422283fb7d1e",
    "Labels": {
        "com.docker/engine.bundle.path": "/var/run/docker/containerd/bf4ea5bdd32301e4a7f81b39ea157d37e0b992306c6605fa2f52422283fb7d1e"
    },
    "Image": "docker.io/library/alpine:latest",
    "Runtime": {
        "Name": "io.containerd.runc.v2",
        "Options": {
            "type_url": "containerd.runc.v1.Options",
            "value": "MgRydW5jOhwvdmFyL3J1bi9kb2NrZXIvcnVudGltZS1ydW5j"
        }
    },
    "SnapshotKey": "",
    "Snapshotter": "",
    "CreatedAt": "2026-09-08T12:00:21.994801991Z",
    "UpdatedAt": "2026-09-08T12:00:21.994801991Z",
    "Extensions": {},
    "SandboxID": ""
}"#;

/// A container created through containerd rather than through docker, which is what a
/// `nerdctl` or kubelet container looks like: it has a snapshotter, a key, and a name for an
/// id rather than a hex string.
const NERDCTL_CONTAINER_INFO: &str = r#"{
    "ID": "web-1",
    "Labels": { "nerdctl/name": "web-1" },
    "Image": "docker.io/library/nginx:1.29",
    "Runtime": { "Name": "io.containerd.runc.v2" },
    "SnapshotKey": "web-1",
    "Snapshotter": "overlayfs",
    "CreatedAt": "2026-09-08T09:15:02.100000000Z",
    "UpdatedAt": "2026-09-08T09:15:02.100000000Z",
    "SandboxID": "e2f1a0b9c8d7"
}"#;

/// `ctr -n moby images ls`, verbatim, padding included.
///
/// **The one read here whose columns cannot be split on whitespace.** `SIZE` is two tokens,
/// `3.9 MiB`, so a positional split puts the platforms where the labels should be. The
/// header's own column offsets are what the rows are sliced by instead.
const IMAGES: &str = "REF                             TYPE                                    DIGEST                                                                  SIZE    PLATFORMS                                                                                              LABELS \ndocker.io/library/alpine:3.22   application/vnd.oci.image.index.v1+json sha256:14358309a308569c32bdc37e2e0e9694be33a9d99e68afb0f5ff33cc1f695dce 3.9 MiB linux/386,linux/amd64,linux/arm/v6,linux/arm/v7,linux/arm64/v8,linux/ppc64le,linux/riscv64,linux/s390x -      \ndocker.io/library/alpine:latest application/vnd.oci.image.index.v1+json sha256:28bd5fe8b56d1bd048e5babf5b10710ebe0bae67db86916198a6eec434943f8b 4.0 MiB linux/386,linux/amd64,linux/arm/v6,linux/arm/v7,linux/arm64/v8,linux/ppc64le,linux/riscv64,linux/s390x -      \n";

/// The same for a namespace holding none, which is the header alone.
const NO_IMAGES: &str = "REF    TYPE    DIGEST    SIZE    PLATFORMS    LABELS \n";

/// `ctr -n moby tasks ls`, which has no `--quiet` worth using: the pid and the status are
/// the reason to read it, and only the table carries them.
const TASKS: &str =
    "TASK                                                                PID     STATUS    
bf4ea5bdd32301e4a7f81b39ea157d37e0b992306c6605fa2f52422283fb7d1e    3175    RUNNING
";

/// The same, for a namespace whose one container is defined and not running.
const NO_TASKS: &str = "TASK    PID    STATUS    
";

/// `ctr namespaces ls --quiet`, one namespace per line.
///
/// `moby` is docker's, and it is the only one on a plain docker box. A kubernetes node has
/// `k8s.io` beside it, and anything using `nerdctl` gets `default`.
const NAMESPACES: &str = "moby\nk8s.io\n";

/// `ctr --version`, which needs no socket and is how the client's own version is read.
const CLIENT_VERSION: &str = "ctr github.com/containerd/containerd/v2 v2.3.4";

/// The socket the fixtures pretend to be behind, which no test connects to.
const ADDRESS: &str = "/run/docker/containerd/containerd.sock";

/// What one fake containerd answers with, per namespace.
struct NamespaceFixtures<'a> {
    name: &'a str,
    /// The containers `containers ls --quiet` lists, each with its `info` document. An id
    /// listed with no document drives the container that went away mid-read.
    containers: &'a [(&'a str, Option<&'a str>)],
    /// What `tasks ls` prints for the namespace.
    tasks: &'a str,
    /// What `images ls` prints for it.
    images: &'a str,
}

/// A containerd whose namespaces are all empty, for the tests about the engine itself.
///
/// Empty rather than absent: the source reads every namespace it was told about, so a
/// fixture that listed a namespace and then answered nothing for it would be a shim that
/// lies about the box rather than one that stands in for it.
fn fake_containerd(name: &str, version: &str, namespaces: &str) -> Containerd {
    let empty: Vec<NamespaceFixtures> = namespaces
        .lines()
        .map(str::trim)
        .filter(|namespace| !namespace.is_empty())
        .map(|namespace| NamespaceFixtures {
            name: namespace,
            containers: &[],
            tasks: NO_TASKS,
            images: NO_IMAGES,
        })
        .collect();

    fake_containerd_holding(name, version, namespaces, &empty)
}

fn fake_containerd_holding(
    name: &str,
    version: &str,
    namespaces: &str,
    per_namespace: &[NamespaceFixtures],
) -> Containerd {
    let root = scratch_tree(&format!("containerd-{name}"), &[]);
    let directory = root.to_str().expect("a UTF-8 scratch path");
    let path = root.join("ctr");
    fs::write(
        &path,
        format!(
            r#"#!/bin/sh
# The arguments are scanned rather than positional: `ctr` takes `--version` on its own and
# every subcommand behind an address flag.
namespace=''
subcommand=''
action=''
target=''
while [ $# -gt 0 ]; do
case "$1" in
--version)
printf '%s\n' '{CLIENT_VERSION}'
exit 0
;;
-a) shift ;;
-n) namespace="$2"; shift ;;
version)
cat <<'STDOUT'
{version}
STDOUT
exit 0
;;
namespaces)
cat <<'STDOUT'
{namespaces}
STDOUT
exit 0
;;
containers|tasks|images)
subcommand="$1"
action="$2"
target="$3"
break
;;
esac
shift
done

case "$subcommand:$action" in
containers:ls)
cat '{directory}/ids-'"$namespace"
;;
containers:info)
document='{directory}/info-'"$namespace"'-'"$target"'.json'
if [ -f "$document" ]; then
cat "$document"
else
printf 'ctr: container %s not found\n' "$target" >&2
exit 1
fi
;;
tasks:ls)
cat '{directory}/tasks-'"$namespace"
;;
images:ls)
cat '{directory}/images-'"$namespace"
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

    for namespace in per_namespace {
        let mut ids = String::new();
        for (id, document) in namespace.containers {
            ids.push_str(id);
            ids.push('\n');
            if let Some(document) = document {
                fs::write(
                    root.join(format!("info-{}-{id}.json", namespace.name)),
                    document,
                )
                .expect("a writable fixture");
            }
        }
        fs::write(root.join(format!("ids-{}", namespace.name)), &ids).expect("a writable fixture");
        fs::write(
            root.join(format!("tasks-{}", namespace.name)),
            namespace.tasks,
        )
        .expect("a writable fixture");
        fs::write(
            root.join(format!("images-{}", namespace.name)),
            namespace.images,
        )
        .expect("a writable fixture");
    }

    let tool = CanonicalTool::located_in("ctr", &[directory]).expect("the fake tool is locatable");

    Containerd::using(tool, Some(ADDRESS.to_owned()))
}

fn containerd_facet(name: &str, version: &str, namespaces: &str) -> Observation {
    ContainersCollector::reading(vec![EngineSource::Containerd(fake_containerd(
        name, version, namespaces,
    ))])
    .collect()
    .expect("the fixtures are well formed")
}

#[test]
fn presence_is_present_when_containerd_is_on_the_host() {
    // Arrange
    let collector = ContainersCollector::reading(vec![EngineSource::Containerd(fake_containerd(
        "present", VERSION, NAMESPACES,
    ))]);

    // Act & Assert
    assert_eq!(collector.presence(), Presence::Present);
}

#[test]
fn the_facet_holds_containerd_under_its_own_key() {
    // Arrange: a box with both engines has both, because docker runs containerd underneath
    // itself and the two describe the same containers at different levels.
    let observed = containerd_facet("keyed", VERSION, NAMESPACES);

    // Act & Assert
    assert_eq!(keys_of(&observed), vec!["containerd".to_owned()]);
}

#[test]
fn containerd_reports_the_client_and_the_server_it_reached() {
    // Arrange: the revision matters more here than for docker. containerd's version moves
    // slowly and the revision is what a distribution's rebuild changes.
    let containerd = field(
        &containerd_facet("versions", VERSION, NAMESPACES),
        "containerd",
    );
    let server = field(&containerd, "server");

    // Act & Assert
    assert_eq!(text(&field(&containerd, "client_version")), "v2.3.4");
    assert_eq!(text(&field(&containerd, "daemon")), "answering");
    assert_eq!(text(&field(&server, "version")), "v2.3.4");
    assert_eq!(
        text(&field(&server, "revision")),
        "db8809540e1a7a9da5d518876894933ff55692ab"
    );
}

#[test]
fn the_address_it_was_reached_at_is_recorded() {
    // Arrange: the value that says which containerd this is. On a docker box it is under
    // docker's own runtime directory rather than at containerd's default, so the address
    // distinguishes a containerd docker manages from one the operator runs.
    let server = field(
        &field(
            &containerd_facet("address", VERSION, NAMESPACES),
            "containerd",
        ),
        "server",
    );

    // Act & Assert
    assert_eq!(text(&field(&server, "address")), ADDRESS);
}

#[test]
fn the_namespaces_are_keyed_by_name() {
    // Arrange: a namespace is containerd's tenancy boundary, and which ones exist says who
    // is using it: `moby` is docker's, `k8s.io` is a kubelet's, `default` is nerdctl's. It
    // is the outer key rather than a field on each container, because two namespaces may
    // hold containers with the same id and nothing in one is visible from the other.
    let server = field(
        &field(
            &containerd_facet("namespaces", VERSION, NAMESPACES),
            "containerd",
        ),
        "server",
    );

    // Act & Assert
    assert_eq!(
        keys_of(&field(&server, "namespaces")),
        vec!["k8s.io".to_owned(), "moby".to_owned()]
    );
}

#[test]
fn a_containerd_with_no_address_to_reach_is_installed_and_unreachable() {
    // Arrange: `ctr` is on the box and no containerd is running behind it, which is state
    // rather than a failed read, and the same shape docker's dead daemon gets.
    let root = scratch_tree("containerd-no-address", &[]);
    let path = root.join("ctr");
    // Answers `--version` and nothing else, which is what a `ctr` with no containerd behind
    // it does: measured on 2.3.4, the `version` subcommand exits non-zero and prints
    // nothing, while `--version` never connects and answers.
    fs::write(
        &path,
        format!("#!/bin/sh\ncase \"$1\" in\n--version) printf '%s\\n' '{CLIENT_VERSION}';;\n*) exit 1;;\nesac\n"),
    )
    .expect("a writable script");
    let mut permissions = fs::metadata(&path).expect("metadata").permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&path, permissions).expect("an executable script");
    let tool = CanonicalTool::located_in("ctr", &[root.to_str().expect("utf-8")])
        .expect("the fake tool is locatable");

    // Act
    let observed = ContainersCollector::reading(vec![EngineSource::Containerd(Containerd::using(
        tool, None,
    ))])
    .collect()
    .expect("an unreachable containerd is not a failed read");
    let containerd = field(&observed, "containerd");

    // Assert
    assert_eq!(text(&field(&containerd, "client_version")), "v2.3.4");
    assert_eq!(text(&field(&containerd, "daemon")), "unreachable");
    assert!(text(&field(&containerd, "daemon_reason")).contains("no containerd"));
    assert!(is_null(&field(&containerd, "server")));
}

#[test]
fn output_that_carries_no_server_block_fails_the_facet() {
    // Arrange: a `ctr` that cannot reach containerd exits non-zero and prints nothing, so a
    // *successful* run with no server block in it does not mean nothing answered — it means
    // the output is not the shape rastro reads, and that is the day this has to be loud.
    let collector = ContainersCollector::reading(vec![EngineSource::Containerd(fake_containerd(
        "garbage",
        "ctr version 2.3.4\n",
        NAMESPACES,
    ))]);

    // Act
    let failure = collector.collect().expect_err("one line is not two blocks");

    // Assert
    assert!(
        failure.to_string().contains("ctr version"),
        "the failure should name the read that produced it: {failure}"
    );
}

fn namespace_of(name: &str, per_namespace: &[NamespaceFixtures], namespace: &str) -> Observation {
    let observed = ContainersCollector::reading(vec![EngineSource::Containerd(
        fake_containerd_holding(name, VERSION, NAMESPACES, per_namespace),
    )])
    .collect()
    .expect("the fixtures are well formed");

    field(
        &field(
            &field(&field(&observed, "containerd"), "server"),
            "namespaces",
        ),
        namespace,
    )
}

/// The two namespaces the fixtures describe: docker's, holding one running container, and a
/// kubelet-shaped one holding a container that is defined and not running.
fn both_namespaces() -> Vec<NamespaceFixtures<'static>> {
    vec![
        NamespaceFixtures {
            name: "moby",
            containers: &[(
                "bf4ea5bdd32301e4a7f81b39ea157d37e0b992306c6605fa2f52422283fb7d1e",
                Some(CONTAINER_INFO),
            )],
            tasks: TASKS,
            images: IMAGES,
        },
        NamespaceFixtures {
            name: "k8s.io",
            containers: &[("web-1", Some(NERDCTL_CONTAINER_INFO))],
            tasks: NO_TASKS,
            images: NO_IMAGES,
        },
    ]
}

#[test]
fn a_namespaces_containers_are_keyed_by_id() {
    // Arrange: keyed by id rather than by name, unlike docker's containers, because
    // containerd has no names. A container's id is whatever created it chose: docker and a
    // kubelet use a hex string, `nerdctl` uses the name the operator typed.
    let namespace = namespace_of("keyed-containers", &both_namespaces(), "k8s.io");

    // Act & Assert
    assert_eq!(
        keys_of(&field(&namespace, "containers")),
        vec!["web-1".to_owned()]
    );
}

#[test]
fn a_container_records_the_runtime_and_the_image_containerd_holds_for_it() {
    // Arrange: the layer underneath docker's account. Which OCI runtime runs a container is
    // containerd's to say, and nothing in the docker entry does.
    let container = field(
        &field(
            &namespace_of("runtime", &both_namespaces(), "moby"),
            "containers",
        ),
        "bf4ea5bdd32301e4a7f81b39ea157d37e0b992306c6605fa2f52422283fb7d1e",
    );

    // Act & Assert
    assert_eq!(text(&field(&container, "runtime")), "io.containerd.runc.v2");
    assert_eq!(
        text(&field(&container, "image")),
        "docker.io/library/alpine:latest"
    );
    assert_eq!(
        text(&field(&container, "created")),
        "2026-09-08T12:00:21.994801991Z"
    );
}

#[test]
fn a_container_docker_manages_has_no_snapshotter_of_its_own() {
    // Arrange: docker keeps its own snapshots and hands containerd a prepared rootfs, so
    // both fields come back empty. Empty text would claim a snapshotter called nothing.
    let container = field(
        &field(
            &namespace_of("snapshotter", &both_namespaces(), "moby"),
            "containers",
        ),
        "bf4ea5bdd32301e4a7f81b39ea157d37e0b992306c6605fa2f52422283fb7d1e",
    );

    // Act & Assert
    assert!(is_null(&field(&container, "snapshotter")));
    assert!(is_null(&field(&container, "snapshot_key")));
}

#[test]
fn a_container_created_through_containerd_records_its_snapshot_and_sandbox() {
    // Arrange: what a `nerdctl` or kubelet container looks like. The sandbox is the value a
    // kubernetes node needs, since it is what ties a container to its pod.
    let container = field(
        &field(
            &namespace_of("nerdctl", &both_namespaces(), "k8s.io"),
            "containers",
        ),
        "web-1",
    );

    // Act & Assert
    assert_eq!(text(&field(&container, "snapshotter")), "overlayfs");
    assert_eq!(text(&field(&container, "snapshot_key")), "web-1");
    assert_eq!(text(&field(&container, "sandbox")), "e2f1a0b9c8d7");
}

#[test]
fn a_running_container_carries_the_task_that_is_running_it() {
    // Arrange: in containerd a container is a definition and a task is it running, so the
    // pid and the status live on the task rather than on the container.
    let task = field(
        &field(
            &field(
                &namespace_of("task", &both_namespaces(), "moby"),
                "containers",
            ),
            "bf4ea5bdd32301e4a7f81b39ea157d37e0b992306c6605fa2f52422283fb7d1e",
        ),
        "task",
    );

    // Act & Assert
    assert_eq!(text(&field(&task, "status")), "RUNNING");
    assert_eq!(integer(&field(&task, "process_id")), 3175);
    assert_eq!(
        field(&task, "process_id").volatility(),
        Volatility::Volatile,
        "a pid moves whenever the process is replaced"
    );
    assert_eq!(field(&task, "status").volatility(), Volatility::Stable);
}

#[test]
fn a_container_with_no_task_is_defined_and_not_running() {
    // Arrange: the state that has no equivalent in docker's account, where a container
    // always carries a status. Here the absence of a task *is* the status.
    let container = field(
        &field(
            &namespace_of("taskless", &both_namespaces(), "k8s.io"),
            "containers",
        ),
        "web-1",
    );

    // Act & Assert
    assert!(is_null(&field(&container, "task")));
}

#[test]
fn a_container_that_vanished_while_being_read_is_recorded_per_namespace() {
    // Arrange: the same race docker's containers have, and the same treatment, kept inside
    // the namespace it happened in.
    let namespaces = vec![
        NamespaceFixtures {
            name: "moby",
            containers: &[
                (
                    "bf4ea5bdd32301e4a7f81b39ea157d37e0b992306c6605fa2f52422283fb7d1e",
                    Some(CONTAINER_INFO),
                ),
                ("gone-1", None),
            ],
            tasks: TASKS,
            images: IMAGES,
        },
        // Empty, and listed all the same: the fixture's namespace list names it, so the
        // shim has to answer for it.
        NamespaceFixtures {
            name: "k8s.io",
            containers: &[],
            tasks: NO_TASKS,
            images: NO_IMAGES,
        },
    ];

    // Act
    let namespace = namespace_of("vanished", &namespaces, "moby");
    let unreadable = field(&namespace, "unreadable_containers");

    // Assert
    assert_eq!(keys_of(&field(&namespace, "containers")).len(), 1);
    assert_eq!(unreadable.volatility(), Volatility::Volatile);
    let entries = items_of(&unreadable);
    assert_eq!(text(&field(&entries[0], "id")), "gone-1");
    assert!(text(&field(&entries[0], "reason")).contains("not found"));
}

#[test]
fn a_namespaces_images_are_keyed_by_reference() {
    // Arrange: containerd's images are named by reference and nothing else, so the
    // reference is the key. Unlike docker there is no separate image id to prefer.
    let images = field(
        &namespace_of("images", &both_namespaces(), "moby"),
        "images",
    );

    // Act & Assert
    assert_eq!(
        keys_of(&images),
        vec![
            "docker.io/library/alpine:3.22".to_owned(),
            "docker.io/library/alpine:latest".to_owned()
        ]
    );
}

#[test]
fn the_columns_are_sliced_by_the_headers_offsets_so_a_two_word_size_shifts_nothing() {
    // Arrange: **the test this parser exists for.** `SIZE` prints as `3.9 MiB`, two
    // whitespace-separated tokens in one column, so splitting the row on whitespace puts the
    // platforms where the labels belong and the digest is the value that proves it did not.
    let image = field(
        &field(
            &namespace_of("offsets", &both_namespaces(), "moby"),
            "images",
        ),
        "docker.io/library/alpine:latest",
    );

    // Act & Assert
    assert_eq!(
        text(&field(&image, "digest")),
        "sha256:28bd5fe8b56d1bd048e5babf5b10710ebe0bae67db86916198a6eec434943f8b"
    );
    assert_eq!(
        text(&field(&image, "media_type")),
        "application/vnd.oci.image.index.v1+json"
    );
}

#[test]
fn an_images_platforms_are_recorded_sorted() {
    // Arrange: a manifest index carries one per architecture, and which ones an image has
    // decides whether it can run on this box at all. The engine prints them in the index's
    // order, which is not one it promises.
    let image = field(
        &field(
            &namespace_of("platforms", &both_namespaces(), "moby"),
            "images",
        ),
        "docker.io/library/alpine:3.22",
    );

    // Act & Assert
    let platforms: Vec<String> = items_of(&field(&image, "platforms"))
        .iter()
        .map(text)
        .collect();
    assert!(platforms.contains(&"linux/arm64/v8".to_owned()));
    let mut sorted = platforms.clone();
    sorted.sort();
    assert_eq!(platforms, sorted);
}

#[test]
fn a_containerd_image_records_no_size_because_the_only_figure_is_rounded() {
    // Arrange: `ctr` prints `3.9 MiB`, a rounded human string, and there is no `images info`
    // to ask for bytes. Recording the rounding would put a number in a diffable document
    // that changes when the formatting does and not when the image does.
    let image = field(
        &field(
            &namespace_of("no-size", &both_namespaces(), "moby"),
            "images",
        ),
        "docker.io/library/alpine:3.22",
    );

    // Act & Assert
    assert_eq!(
        keys_of(&image),
        vec![
            "digest".to_owned(),
            "media_type".to_owned(),
            "platforms".to_owned()
        ]
    );
}

#[test]
fn a_namespace_holding_no_images_records_an_empty_map() {
    // Arrange: the header alone, which is what `ctr` prints for an empty namespace.
    let namespace = namespace_of("no-images", &both_namespaces(), "k8s.io");

    // Act & Assert
    assert!(keys_of(&field(&namespace, "images")).is_empty());
}

/// A containerd whose store and runtime state are the two directories the test named.
fn containerd_holding(name: &str, root: Option<&str>, state: Option<&str>) -> Containerd {
    // Named `scratch` rather than `root`, which is the parameter naming containerd's own
    // store: two different roots in one function is exactly how a shadowed name misleads.
    let scratch = scratch_tree(&format!("containerd-trees-{name}"), &[]);
    let path = scratch.join("ctr");
    fs::write(
        &path,
        format!("#!/bin/sh\ncase \"$1\" in\n--version) printf '%s\\n' '{CLIENT_VERSION}';;\n*) exit 1;;\nesac\n"),
    )
    .expect("a writable script");
    let mut permissions = fs::metadata(&path).expect("metadata").permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&path, permissions).expect("an executable script");
    let tool = CanonicalTool::located_in("ctr", &[scratch.to_str().expect("utf-8")])
        .expect("the fake tool is locatable");

    Containerd::holding(
        tool,
        None,
        root.map(str::to_owned),
        state.map(str::to_owned),
    )
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
fn containerd_seals_its_store_and_its_runtime_state() {
    // Arrange: two claims rather than a listing, which is where this differs from docker.
    // Nothing under either tree belongs to the operator: one is the content store and the
    // snapshots, the other the shims and sockets of running tasks.
    let collector =
        ContainersCollector::reading(vec![EngineSource::Containerd(containerd_holding(
            "sealed",
            Some("/var/lib/containerd"),
            Some("/run/containerd"),
        ))]);

    // Act & Assert
    assert_eq!(
        claimed(&collector),
        vec![
            ("/run/containerd".to_owned(), ClaimedReading::Sealed),
            ("/var/lib/containerd".to_owned(), ClaimedReading::Sealed)
        ]
    );
}

#[test]
fn one_tree_two_engines_resolved_to_is_claimed_once() {
    // Arrange: **the guard exists because a repeat fails the walk rather than a facet.** On
    // a docker box the managed containerd keeps its store inside docker's own root, so two
    // dialects legitimately resolving to one directory is a real arrangement, and saying the
    // same thing about it twice is not a disagreement.
    let collector = ContainersCollector::reading(vec![
        EngineSource::Containerd(containerd_holding(
            "first",
            Some("/var/lib/containerd"),
            None,
        )),
        EngineSource::Containerd(containerd_holding(
            "second",
            Some("/var/lib/containerd"),
            None,
        )),
    ]);

    // Act & Assert
    assert_eq!(
        claimed(&collector),
        vec![("/var/lib/containerd".to_owned(), ClaimedReading::Sealed)]
    );
}

#[test]
fn a_tree_inside_one_already_sealed_is_not_claimed_again() {
    // Arrange: **on a docker box this is the ordinary case.** docker's managed containerd
    // keeps its store at `/var/lib/docker/containerd/daemon`, inside docker's own root, and
    // the walk prunes at the parent — so the deeper rule could never be consulted and would
    // sit in the effective table matching nothing. Where those directories are is reported
    // as state by this facet instead, so folding the rule loses nothing.
    let collector = ContainersCollector::reading(vec![
        EngineSource::Containerd(containerd_holding(
            "outer",
            Some("/var/lib/docker/containerd"),
            None,
        )),
        EngineSource::Containerd(containerd_holding(
            "inner",
            Some("/var/lib/docker/containerd/daemon"),
            None,
        )),
    ]);

    // Act & Assert
    assert_eq!(
        claimed(&collector),
        vec![(
            "/var/lib/docker/containerd".to_owned(),
            ClaimedReading::Sealed
        )]
    );
}

#[test]
fn a_neighbour_with_a_longer_name_is_not_mistaken_for_a_child() {
    // Arrange: `/var/lib/containerdx` is not inside `/var/lib/containerd`, and a prefix
    // comparison would fold away a real claim over somebody else's tree.
    let collector = ContainersCollector::reading(vec![
        EngineSource::Containerd(containerd_holding(
            "short",
            Some("/var/lib/containerd"),
            None,
        )),
        EngineSource::Containerd(containerd_holding(
            "long",
            Some("/var/lib/containerdx"),
            None,
        )),
    ]);

    // Act & Assert
    assert_eq!(
        claimed(&collector)
            .iter()
            .map(|(tree, _)| tree.clone())
            .collect::<Vec<String>>(),
        vec![
            "/var/lib/containerd".to_owned(),
            "/var/lib/containerdx".to_owned()
        ]
    );
}

#[test]
fn containerd_reports_where_it_keeps_what_it_holds() {
    // Arrange: reported as state and not only claimed, because the claim over them can be
    // folded away when a docker root already seals the tree they sit in.
    let root = scratch_tree("containerd-reported-dirs", &[]);
    let path = root.join("ctr");
    fs::write(
        &path,
        format!(
            r#"#!/bin/sh
for argument in "$@"; do
case "$argument" in
--version) printf '%s\n' '{CLIENT_VERSION}'; exit 0;;
version)
cat <<'STDOUT'
{VERSION}
STDOUT
exit 0
;;
namespaces) printf 'moby\n'; exit 0;;
containers) printf ''; exit 0;;
tasks) printf '%s\n' 'TASK    PID    STATUS'; exit 0;;
images) printf '%s\n' 'REF    TYPE    DIGEST    SIZE    PLATFORMS    LABELS'; exit 0;;
esac
done
exit 1
"#
        ),
    )
    .expect("a writable script");
    let mut permissions = fs::metadata(&path).expect("metadata").permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&path, permissions).expect("an executable script");
    let tool = CanonicalTool::located_in("ctr", &[root.to_str().expect("utf-8")])
        .expect("the fake tool is locatable");
    let containerd = Containerd::holding(
        tool,
        Some(ADDRESS.to_owned()),
        Some("/var/lib/docker/containerd/daemon".to_owned()),
        Some("/run/docker/containerd/daemon".to_owned()),
    );

    // Act
    let observed = ContainersCollector::reading(vec![EngineSource::Containerd(containerd)])
        .collect()
        .expect("the fixtures are well formed");
    let server = field(&field(&observed, "containerd"), "server");

    // Assert
    assert_eq!(
        text(&field(&server, "root")),
        "/var/lib/docker/containerd/daemon"
    );
    assert_eq!(
        text(&field(&server, "state")),
        "/run/docker/containerd/daemon"
    );
}
