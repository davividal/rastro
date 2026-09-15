//! Where podman keeps what it holds, read from its configuration rather than from podman.
//!
//! Its own test file for the reason `containerd_layout.rs` is: the discovery has several
//! paths and needs neither a facet nor an engine to exercise. It matters more here, because
//! this is the *only* thing rastro can learn about podman on a box with no service running —
//! asking podman itself would make it initialise a store, which is what
//! `docs/decisions.md` refuses.

mod support;

use std::fs;

use rastro::collectors::containers::PodmanLayout;
use support::fs_tree::{scratch_tree, write};

/// A filesystem root holding the two files podman reads, with the contents given.
///
/// Both are real paths from a podman box: the distributed defaults ship in
/// `/usr/share/containers`, and the operator's overrides go in `/etc/containers`.
fn box_with(name: &str, distributed: &str, operator: &str) -> std::path::PathBuf {
    let root = scratch_tree(&format!("podman-layout-{name}"), &[]);

    if !distributed.is_empty() {
        write(&root, "usr/share/containers/storage.conf", distributed);
    }
    if !operator.is_empty() {
        write(&root, "etc/containers/storage.conf", operator);
    }

    root
}

#[test]
fn the_roots_come_from_the_operators_configuration() {
    // Arrange: the case this exists for. An operator who moved the store onto another disk
    // has moved the thing worth not walking, and nothing else on the box says where.
    let root = box_with(
        "operator",
        "[storage]\ndriver = \"overlay\"\n",
        "[storage]\ndriver = \"overlay\"\ngraphroot = \"/srv/containers/storage\"\n\
         runroot = \"/run/srv-containers\"\n",
    );

    // Act
    let layout = PodmanLayout::under(&root);

    // Assert
    assert_eq!(
        layout.graph_root.map(|path| path.as_str().to_owned()),
        Some("/srv/containers/storage".to_owned())
    );
    assert_eq!(
        layout.run_root.map(|path| path.as_str().to_owned()),
        Some("/run/srv-containers".to_owned())
    );
}

#[test]
fn the_distributed_defaults_are_read_when_the_operator_overrides_nothing() {
    // Arrange: a distribution may ship a graphroot of its own, and an operator's file that
    // sets only the driver must not silently discard it.
    let root = box_with(
        "distributed",
        "[storage]\ngraphroot = \"/var/lib/distro/storage\"\n",
        "[storage]\ndriver = \"overlay\"\n",
    );

    // Act & Assert
    assert_eq!(
        PodmanLayout::under(&root)
            .graph_root
            .map(|path| path.as_str().to_owned()),
        Some("/var/lib/distro/storage".to_owned())
    );
}

#[test]
fn the_documented_defaults_apply_when_neither_file_sets_them() {
    // Arrange: **the ordinary case, measured.** Neither `/usr/share/containers/storage.conf`
    // nor `/etc/containers/storage.conf` names a root on a stock podman box, and podman
    // resolved them to these two paths.
    let root = box_with(
        "defaults",
        "[storage]\n[storage.options]\n[storage.options.overlay]\nmountopt = \"nodev\"\n",
        "",
    );

    // Act
    let layout = PodmanLayout::under(&root);

    // Assert
    assert_eq!(
        layout.graph_root.map(|path| path.as_str().to_owned()),
        Some("/var/lib/containers/storage".to_owned())
    );
    assert_eq!(
        layout.run_root.map(|path| path.as_str().to_owned()),
        Some("/run/containers/storage".to_owned())
    );
}

#[test]
fn the_volume_tree_defaults_to_one_inside_the_graph_root() {
    // Arrange: podman resolved it to `<graphroot>/volumes` on the reference box, and it is
    // the one tree under the store that holds the operator's own data.
    let root = box_with("volumes", "[storage]\n", "");

    // Act & Assert
    assert_eq!(
        PodmanLayout::under(&root)
            .volume_path
            .map(|path| path.as_str().to_owned()),
        Some("/var/lib/containers/storage/volumes".to_owned())
    );
}

#[test]
fn a_volume_tree_moved_out_of_the_store_is_read_from_the_engine_configuration() {
    // Arrange: `volume_path` lives in `containers.conf` rather than `storage.conf`, and an
    // operator who moved it has moved the one tree the claim must spare.
    let root = box_with("moved-volumes", "[storage]\n", "");
    fs::create_dir_all(root.join("etc/containers")).expect("a writable fixture");
    write(
        &root,
        "etc/containers/containers.conf",
        "[engine]\nvolume_path = \"/srv/volumes\"\n",
    );

    // Act & Assert
    assert_eq!(
        PodmanLayout::under(&root)
            .volume_path
            .map(|path| path.as_str().to_owned()),
        Some("/srv/volumes".to_owned())
    );
}

#[test]
fn a_box_with_no_configuration_at_all_still_has_the_documented_defaults() {
    // Arrange: podman installed from a tarball leaves neither file, and the defaults are
    // compiled into podman rather than written anywhere.
    let root = scratch_tree("podman-layout-bare", &[]);

    // Act & Assert
    assert_eq!(
        PodmanLayout::under(&root)
            .graph_root
            .map(|path| path.as_str().to_owned()),
        Some("/var/lib/containers/storage".to_owned())
    );
}

#[test]
fn a_configuration_that_will_not_parse_falls_back_rather_than_guessing_wrong() {
    // Arrange: a half-written config is a box somebody is in the middle of changing. The
    // documented defaults are where podman will look if it cannot read the file either, and
    // a claim against them is better than a claim against a path invented from half a file.
    let root = box_with("broken", "", "[storage\ngraphroot = \"/srv/broken\n");

    // Act & Assert
    assert_eq!(
        PodmanLayout::under(&root)
            .graph_root
            .map(|path| path.as_str().to_owned()),
        Some("/var/lib/containers/storage".to_owned())
    );
}

#[test]
fn a_rootless_layout_is_read_from_the_users_own_configuration() {
    // Arrange: **a rootless engine is configured somewhere else and defaults somewhere
    // else.** Its store is under the user's home rather than in `/var/lib`, its runtime
    // state is in their runtime directory, and `~/.config/containers` overrides the system
    // files rather than being overridden by them.
    let home = scratch_tree("podman-rootless-configured", &[]);
    write(
        &home,
        ".config/containers/storage.conf",
        "[storage]\ngraphroot = \"/srv/alice/containers\"\n",
    );

    // Act
    let layout = PodmanLayout::for_account(home.to_str().expect("utf-8"), 1000);

    // Assert
    assert_eq!(
        layout.graph_root.map(|path| path.as_str().to_owned()),
        Some("/srv/alice/containers".to_owned())
    );
    assert_eq!(
        layout.run_root.map(|path| path.as_str().to_owned()),
        Some("/run/user/1000/containers".to_owned())
    );
}

#[test]
fn a_rootless_user_who_configured_nothing_is_at_the_per_user_defaults() {
    // Arrange: which are not the system ones. A document that claimed `/var/lib/containers`
    // for alice would be naming root's store as hers.
    let home = scratch_tree("podman-rootless-defaults", &[]);

    // Act
    let layout = PodmanLayout::for_account(home.to_str().expect("utf-8"), 1001);
    let home = home.to_str().expect("utf-8");

    // Assert
    assert_eq!(
        layout.graph_root.map(|path| path.as_str().to_owned()),
        Some(format!("{home}/.local/share/containers/storage"))
    );
    assert_eq!(
        layout.run_root.map(|path| path.as_str().to_owned()),
        Some("/run/user/1001/containers".to_owned())
    );
    assert_eq!(
        layout.volume_path.map(|path| path.as_str().to_owned()),
        Some(format!("{home}/.local/share/containers/storage/volumes"))
    );
}
