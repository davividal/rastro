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
use rastro_collector::{Collector, Presence};
use rastro_fingerprint::Observation;
use support::fs_tree::scratch_tree;
use support::observation::{field, is_null, items_of, keys_of, text};

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

/// `ctr namespaces ls --quiet`, one namespace per line.
///
/// `moby` is docker's, and it is the only one on a plain docker box. A kubernetes node has
/// `k8s.io` beside it, and anything using `nerdctl` gets `default`.
const NAMESPACES: &str = "moby\nk8s.io\n";

/// `ctr --version`, which needs no socket and is how the client's own version is read.
const CLIENT_VERSION: &str = "ctr github.com/containerd/containerd/v2 v2.3.4";

/// The socket the fixtures pretend to be behind, which no test connects to.
const ADDRESS: &str = "/run/docker/containerd/containerd.sock";

fn fake_containerd(name: &str, version: &str, namespaces: &str) -> Containerd {
    let root = scratch_tree(&format!("containerd-{name}"), &[]);
    let directory = root.to_str().expect("a UTF-8 scratch path");
    let path = root.join("ctr");
    fs::write(
        &path,
        format!(
            r#"#!/bin/sh
# The arguments are scanned rather than positional: `ctr` takes `--version` on its own and
# every subcommand behind an address flag.
for argument in "$@"; do
case "$argument" in
--version)
printf '%s\n' '{CLIENT_VERSION}'
exit 0
;;
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
fn the_namespaces_are_recorded_sorted() {
    // Arrange: a namespace is containerd's tenancy boundary, and which ones exist says who
    // is using it: `moby` is docker's, `k8s.io` is a kubelet's, `default` is nerdctl's.
    // They arrive in the engine's order, which is not one it promises.
    let server = field(
        &field(
            &containerd_facet("namespaces", VERSION, NAMESPACES),
            "containerd",
        ),
        "server",
    );

    // Act & Assert
    assert_eq!(
        items_of(&field(&server, "namespaces"))
            .iter()
            .map(text)
            .collect::<Vec<String>>(),
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
