//! rastro's account of a box's containers against the engine's own answer.
//!
//! Every other test of this facet asserts what rastro does with output somebody captured
//! once, which pins the code and cannot catch a fixture that was captured wrong. This one
//! asks docker. It is the same arrangement as `nginx_conformance.rs`, and for the same
//! reason: a test whose expected answer was written by the person who wrote the parser only
//! re-encodes their belief.
//!
//! **It needs a live docker with at least one container, and it says so rather than
//! skipping.** A check that quietly passes on a box with no engine is how a whole dialect
//! could rot unnoticed, so an absent docker and an empty box are both failures here, each
//! naming what to do about it. `.github/workflows/live-engine.yml` provides the engine and
//! the containers in CI; `scripts/test-in-container.sh` cannot, because dockerd inside the
//! suite's container would need privilege the other legs do not have.

use std::collections::BTreeSet;
use std::process::Command;

mod support;

use rastro::collectors::ContainersCollector;
use rastro_collector::{ClaimedReading, Collector};
use rastro_fingerprint::{Content, Observation, Scalar};
use support::observation::{field, keys_of};

const PROGRAM: &str = "docker";

/// The one tree under the engine's root that the claim must never seal.
const OPERATOR_DATA: &str = "volumes";

/// What docker says, or a failure naming how to give this test an engine.
fn docker(arguments: &[&str]) -> String {
    let run = Command::new(PROGRAM)
        .args(arguments)
        .output()
        .unwrap_or_else(|error| {
            panic!(
                "this test asks docker for its own answer, and {PROGRAM} could not be run: \
                 {error}. Install it and start the daemon, or run this target where one is \
                 already running: see .github/workflows/live-engine.yml"
            )
        });

    assert!(
        run.status.success(),
        "`{PROGRAM} {}` failed, so there is no answer to compare against: {}",
        arguments.join(" "),
        String::from_utf8_lossy(&run.stderr)
    );

    String::from_utf8_lossy(&run.stdout).into_owned()
}

/// Every non-empty line of an answer, as a set.
fn lines_of(output: &str) -> BTreeSet<String> {
    output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

/// rastro's own reading of this box, through the real detection.
fn observed() -> Observation {
    let collector = ContainersCollector::new();
    let observation = collector
        .collect()
        .expect("a live docker should be readable; if it is not, that is the finding");

    field(&observation, "docker")
}

fn server() -> Observation {
    let docker = observed();
    let server = field(&docker, "server");

    assert!(
        !matches!(server.content(), Content::Scalar(Scalar::Null)),
        "the daemon did not answer, so there is nothing to compare: rastro said {:?}",
        field(&docker, "daemon_reason").content()
    );

    server
}

#[test]
fn the_container_names_are_the_ones_docker_lists() {
    // Arrange: docker's own `ps` is the oracle. `--all` on both sides, because a stopped
    // container is state and the two lists have to agree about it.
    let theirs = lines_of(&docker(&["ps", "--all", "--format", "{{.Names}}"]));
    assert!(
        !theirs.is_empty(),
        "this test compares two lists of containers and docker has none: start one, or run \
         this target where the workflow has (see .github/workflows/live-engine.yml)"
    );

    // Act
    let ours: BTreeSet<String> = keys_of(&field(&server(), "containers"))
        .into_iter()
        .collect();

    // Assert
    assert_eq!(ours, theirs);
}

#[test]
fn the_image_ids_are_the_ones_docker_lists() {
    // Arrange: untruncated ids on both sides, and `--all` so a dangling image counts.
    let theirs = lines_of(&docker(&["image", "ls", "--all", "--no-trunc", "--quiet"]));
    assert!(
        !theirs.is_empty(),
        "this test compares two lists of images and docker has none: pull one, or run this \
         target where the workflow has"
    );

    // Act
    let ours: BTreeSet<String> = keys_of(&field(&server(), "images")).into_iter().collect();

    // Assert
    assert_eq!(ours, theirs);
}

#[test]
fn the_volume_names_are_the_ones_docker_lists() {
    // Arrange
    let theirs = lines_of(&docker(&["volume", "ls", "--quiet"]));
    assert!(
        !theirs.is_empty(),
        "this test compares two lists of volumes and docker has none: create one, or run \
         this target where the workflow has"
    );

    // Act
    let ours: BTreeSet<String> = keys_of(&field(&server(), "volumes")).into_iter().collect();

    // Assert
    assert_eq!(ours, theirs);
}

#[test]
fn the_sealed_trees_are_the_engines_own_directories_and_never_the_operators_data() {
    // Arrange: **the check the claim exists for, against the engine's own root.** The claim
    // is built by listing that root's children, so docker naming the root is the oracle: the
    // sealed set has to be exactly the directories under it, less the one holding the
    // operator's volumes. Nothing here trusts a directory name rastro holds, which is the
    // whole point — the layer store is `overlay2` on one driver, `vfs` on another and
    // `rootfs` on docker 29.
    let root = docker(&["info", "--format", "{{.DockerRootDir}}"])
        .trim()
        .to_owned();
    assert!(
        !root.is_empty(),
        "docker did not say where its root is, so the claim cannot be checked"
    );

    let mut theirs = BTreeSet::new();
    for entry in std::fs::read_dir(&root).expect("the engine's root should be readable") {
        let entry = entry.expect("a readable directory entry");
        if entry.path().is_dir() && entry.file_name() != OPERATOR_DATA {
            theirs.insert(entry.path().to_string_lossy().into_owned());
        }
    }
    assert!(
        !theirs.is_empty(),
        "the engine's root at {root:?} has no directories in it, so there is nothing to seal"
    );

    // Act
    let claims = ContainersCollector::new().filesystem_claims();
    let ours: BTreeSet<String> = claims
        .iter()
        .filter(|claim| claim.reading() == ClaimedReading::Sealed)
        .map(|claim| claim.tree().as_str().to_owned())
        .filter(|tree| tree.starts_with(&root))
        .collect();

    // Assert
    assert_eq!(ours, theirs);
    assert!(
        !ours.contains(&format!("{root}/{OPERATOR_DATA}")),
        "the operator's volumes must never be sealed: a config can only narrow, so there \
         would be no way to ask for them back"
    );
}
