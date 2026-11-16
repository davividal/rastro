//! How the NFS server is running, and the module parameters of both halves.
//!
//! The values are what Debian 12's 6.1 kernel published with nfs-kernel-server 2.6.2 started
//! at its defaults. Read from a scratch tree standing in for `/proc/fs/nfsd` and `/sys/module`.

mod support;

use std::fs;
use std::os::unix::fs::PermissionsExt;

use rastro::collectors::nfs::{ModuleParameterDirectory, NfsdFilesystem};
use rastro_collector::Observation;
use support::fs_tree::{scratch_tree, write};
use support::observation::{field, is_null, keys_of, text};

#[test]
fn runtime_reports_each_file_less_its_newline() {
    // Arrange
    let root = scratch_tree("nfs_runtime_values", &[]);
    write(&root, "versions", "+3 +4 +4.1 +4.2\n");
    write(&root, "threads", "8\n");
    write(&root, "portlist", "tcp 2049\ntcp 2049\n");

    // Act
    let runtime = Observation::from(&NfsdFilesystem::at(&root).runtime());

    // Assert: a value spanning lines keeps the lines between, as the kernel wrote them.
    assert_eq!(text(&field(&runtime, "versions")), "+3 +4 +4.1 +4.2");
    assert_eq!(text(&field(&runtime, "threads")), "8");
    assert_eq!(text(&field(&runtime, "portlist")), "tcp 2049\ntcp 2049");
}

#[test]
fn runtime_reports_a_file_this_kernel_does_not_publish_as_null() {
    // Arrange: `max_connections` arrived in 4.20, so an older kernel has no such file.
    let root = scratch_tree("nfs_runtime_missing", &[]);
    write(&root, "threads", "8\n");

    // Act
    let runtime = Observation::from(&NfsdFilesystem::at(&root).runtime());

    // Assert: every key is there whatever the kernel, so a consumer never meets one sometimes.
    assert!(is_null(&field(&runtime, "max_connections")));
    assert_eq!(
        keys_of(&runtime),
        vec![
            "max_block_size",
            "max_connections",
            "nfsv4gracetime",
            "nfsv4leasetime",
            "nfsv4recoverydir",
            "pool_threads",
            "portlist",
            "threads",
            "versions",
        ]
    );
}

#[test]
fn runtime_reports_a_file_it_could_not_read_as_an_error() {
    // Arrange: a directory where a value should be, which no account can read as one. Standing in
    // for the `0600` files an unprivileged run is refused, without depending on who runs this.
    let root = scratch_tree("nfs_runtime_refused", &["threads"]);

    // Act
    let runtime = Observation::from(&NfsdFilesystem::at(&root).runtime());

    // Assert: a refused read is never a null, which would read as a file the kernel lacks.
    let threads = field(&runtime, "threads");
    assert!(!is_null(&field(&threads, "error")));
    assert_eq!(runtime.incomplete_items(), 1);
}

#[test]
fn runtime_refuses_a_fifo_without_opening_it() {
    // Arrange: opening a FIFO nobody writes to blocks, so a value read from one would hang the run.
    let root = scratch_tree("nfs_runtime_fifo", &[]);
    let made = std::process::Command::new("mkfifo")
        .arg(root.join("threads"))
        .status()
        .expect("mkfifo should be runnable");
    assert!(made.success());

    // Act
    let runtime = Observation::from(&NfsdFilesystem::at(&root).runtime());

    // Assert
    assert!(!is_null(&field(&field(&runtime, "threads"), "error")));
}

#[test]
fn runtime_leaves_out_what_moves_with_client_traffic() {
    // Arrange
    let root = scratch_tree("nfs_runtime_traffic", &["clients/3"]);
    write(&root, "pool_stats", "# pool packets-arrived\n0 12\n");
    write(&root, "reply_cache_stats", "max entries: 1\n");

    // Act
    let runtime = Observation::from(&NfsdFilesystem::at(&root).runtime());

    // Assert
    let keys = keys_of(&runtime);
    assert!(
        !keys
            .iter()
            .any(|key| key.contains("stats") || key == "clients")
    );
}

#[test]
fn parameters_reports_every_readable_parameter_of_a_loaded_module() {
    // Arrange
    let root = scratch_tree("nfs_parameters_loaded", &["nfs/parameters"]);
    write(&root, "nfs/parameters/nfs4_disable_idmapping", "Y\n");
    write(&root, "nfs/parameters/nfs4_unique_id", "\n");

    // Act
    let parameters = ModuleParameterDirectory::at(&root)
        .parameters_of("nfs")
        .expect("the module is loaded");

    // Assert: an empty value is a parameter never set, and kept as the empty text it is.
    let parameters = Observation::from(&parameters);
    assert_eq!(text(&field(&parameters, "nfs4_disable_idmapping")), "Y");
    assert_eq!(text(&field(&parameters, "nfs4_unique_id")), "");
}

#[test]
fn parameters_leaves_out_one_nobody_may_read() {
    // Arrange: a parameter that is a button, which the kernel marks by granting no read bit.
    let root = scratch_tree("nfs_parameters_write_only", &["nfsd/parameters"]);
    write(&root, "nfsd/parameters/cltrack_prog", "\n");
    write(&root, "nfsd/parameters/trigger", "");
    fs::set_permissions(
        root.join("nfsd/parameters/trigger"),
        fs::Permissions::from_mode(0o200),
    )
    .expect("a scratch file");

    // Act
    let parameters = Observation::from(
        &ModuleParameterDirectory::at(&root)
            .parameters_of("nfsd")
            .expect("the module is loaded"),
    );

    // Assert
    assert_eq!(keys_of(&parameters), vec!["cltrack_prog"]);
}

#[test]
fn parameters_of_a_module_not_loaded_is_none() {
    // Arrange
    let root = scratch_tree("nfs_parameters_unloaded", &[]);

    // Act & Assert
    assert!(
        ModuleParameterDirectory::at(&root)
            .parameters_of("nfsv4")
            .is_none()
    );
}

#[test]
fn parameters_of_a_loaded_module_with_none_is_empty() {
    // Arrange: `nfsv4` is loaded with no `parameters` directory on kernels where it has none.
    let root = scratch_tree("nfs_parameters_none", &["nfsv4"]);

    // Act
    let parameters = ModuleParameterDirectory::at(&root)
        .parameters_of("nfsv4")
        .expect("the module is loaded");

    // Assert
    assert!(keys_of(&Observation::from(&parameters)).is_empty());
}

#[test]
fn runtime_refuses_a_value_past_its_bound() {
    // Arrange: more than any setting the kernel writes, so a misread path costs one value.
    let root = scratch_tree("nfs_runtime_oversized", &[]);
    write(&root, "portlist", &"tcp 2049\n".repeat(10_000));

    // Act
    let runtime = Observation::from(&NfsdFilesystem::at(&root).runtime());

    // Assert
    assert!(text(&field(&field(&runtime, "portlist"), "error")).contains("longer than"));
}

#[test]
fn runtime_refuses_a_value_that_is_not_utf8_rather_than_replacing_it() {
    // Arrange
    let root = scratch_tree("nfs_runtime_not_utf8", &[]);
    fs::write(root.join("nfsv4recoverydir"), b"/var/lib/\xff\n").expect("a scratch file");

    // Act
    let runtime = Observation::from(&NfsdFilesystem::at(&root).runtime());

    // Assert: a replacement character would put a path in the document that is not on the box.
    assert!(text(&field(&field(&runtime, "nfsv4recoverydir"), "error")).contains("UTF-8"));
}

#[test]
fn parameters_of_a_module_whose_directory_cannot_be_listed_is_an_error() {
    // Arrange: a file where the directory should be, which no account can list.
    let root = scratch_tree("nfs_parameters_unlisted", &["nfs"]);
    write(&root, "nfs/parameters", "");

    // Act
    let parameters = Observation::from(
        &ModuleParameterDirectory::at(&root)
            .parameters_of("nfs")
            .expect("the module is loaded"),
    );

    // Assert
    assert!(text(&field(&parameters, "error")).contains("could not list"));
    assert_eq!(parameters.incomplete_items(), 1);
}

#[test]
fn parameters_refuses_a_module_listing_more_than_its_bound() {
    // Arrange: `nfs` has twelve; a thousand and one is not a module's parameters.
    let root = scratch_tree("nfs_parameters_too_many", &["nfs/parameters"]);
    for index in 0..=1_000 {
        write(&root, &format!("nfs/parameters/p{index}"), "1\n");
    }

    // Act
    let parameters = Observation::from(
        &ModuleParameterDirectory::at(&root)
            .parameters_of("nfs")
            .expect("the module is loaded"),
    );

    // Assert
    assert!(text(&field(&parameters, "error")).contains("more than"));
}
