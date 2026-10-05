//! The tree the walk is asked to step back from: a node's data directory.
//!
//! Sealed, for the reason the PostgreSQL and RabbitMQ stores are: measured on 7.17, a node with
//! no request at all moved every index's translog checkpoint and retention-lease file within
//! ninety seconds, so a walk of it cannot be byte-identical across two runs of an unchanged box.
//!
//! **What decides a claim is whether the node's data path is the same directory on the host**,
//! not whether the node shares rastro's mount namespace. Found by review: Elastic's own systemd
//! unit sets `PrivateTmp=true`, on 7.17 and 8.15 alike, so every packaged node has a mount
//! namespace of its own and still keeps its data in the host's `/var/lib/elasticsearch`. A node
//! in a container names a directory in its own image, which is not the host's.

use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};

use rastro::collectors::elasticsearch::{ElasticsearchCollector, HttpClient};
use rastro_collector::{ClaimedReading, Collector};

mod support;

use support::fs_tree::{scratch_tree, write};

/// A box with one node on it, its config and data under a scratch tree the test owns.
struct Box_ {
    proc: PathBuf,
    scratch: PathBuf,
}

impl Box_ {
    /// A node whose root is the host's, its mount namespace the same as rastro's or not.
    fn host_node(name: &str, own_mount_namespace: bool, config_file: &str) -> Self {
        let scratch = scratch_tree(name, &["proc/self/ns", "proc/600/ns", "conf", "home"]);
        let proc = scratch.join("proc");
        let argv = format!(
            "/usr/share/elasticsearch/jdk/bin/java\0-Des.path.home={home}\0\
             -Des.path.conf={conf}\0-Des.distribution.type=tar\0\
             -cp\0lib/*\0org.elasticsearch.bootstrap.Elasticsearch\0",
            home = scratch.join("home").display(),
            conf = scratch.join("conf").display(),
        );
        write(&proc, "600/cmdline", &argv);
        write(&proc, "600/environ", "");
        support::process::started(&proc, "600", "1");
        symlink("/", proc.join("600/root")).expect("a writable fixture");
        symlink("mnt:[4026531841]", proc.join("self/ns/mnt")).expect("a writable fixture");
        let theirs = match own_mount_namespace {
            true => "mnt:[4026532777]",
            false => "mnt:[4026531841]",
        };
        symlink(theirs, proc.join("600/ns/mnt")).expect("a writable fixture");
        write(&scratch, "conf/elasticsearch.yml", config_file);

        Self { proc, scratch }
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.scratch.join(relative)
    }

    fn directory(&self, relative: &str) -> PathBuf {
        let path = self.path(relative);
        fs::create_dir_all(&path).expect("a writable fixture");
        path
    }

    fn claimed_trees(&self) -> Vec<String> {
        claimed_trees(&self.proc)
    }
}

fn claimed_trees(proc: &Path) -> Vec<String> {
    ElasticsearchCollector::reading(proc, false, HttpClient::new())
        .filesystem_claims()
        .iter()
        .inspect(|claim| assert_eq!(claim.reading(), ClaimedReading::Sealed))
        .map(|claim| claim.tree().as_str().to_owned())
        .collect()
}

fn data_setting(paths: &[&Path]) -> String {
    let listed: Vec<String> = paths
        .iter()
        .map(|path| path.display().to_string())
        .collect();
    format!("path.data: [{}]\n", listed.join(", "))
}

#[test]
fn filesystem_claims_seal_the_data_directory_the_settings_name() {
    // Arrange: a package install sets it in `elasticsearch.yml`.
    let host = Box_::host_node("elasticsearch-claims-configured", false, "");
    let data = host.directory("var-lib-elasticsearch");
    write(
        &host.scratch,
        "conf/elasticsearch.yml",
        &data_setting(&[&data]),
    );

    // Act & Assert
    assert_eq!(host.claimed_trees(), [data.display().to_string()]);
}

#[test]
fn filesystem_claims_seal_a_packaged_nodes_data_in_its_own_mount_namespace() {
    // Arrange: the packaged unit's `PrivateTmp=true` gives the node a mount namespace of its own,
    // and its data directory is still the host's.
    let host = Box_::host_node("elasticsearch-claims-private-tmp", true, "");
    let data = host.directory("var-lib-elasticsearch");
    write(
        &host.scratch,
        "conf/elasticsearch.yml",
        &data_setting(&[&data]),
    );

    // Act & Assert
    assert_eq!(host.claimed_trees(), [data.display().to_string()]);
}

#[test]
fn filesystem_claims_seal_every_directory_of_a_multi_path_node() {
    // Arrange: deprecated since 7.13 and still accepted.
    let host = Box_::host_node("elasticsearch-claims-several", false, "");
    let first = host.directory("es-a");
    let second = host.directory("es-b");
    write(
        &host.scratch,
        "conf/elasticsearch.yml",
        &data_setting(&[&first, &second]),
    );

    // Act & Assert
    assert_eq!(
        host.claimed_trees(),
        [first.display().to_string(), second.display().to_string()]
    );
}

#[test]
fn filesystem_claims_seal_the_default_under_the_home_where_nothing_names_one() {
    // Arrange: a tarball install, whose data directory is `data` under its home.
    let host = Box_::host_node("elasticsearch-claims-default", false, "");
    let data = host.directory("home/data");

    // Act & Assert
    assert_eq!(host.claimed_trees(), [data.display().to_string()]);
}

#[test]
fn filesystem_claims_resolve_a_relative_path_against_the_home() {
    // Arrange
    let host = Box_::host_node(
        "elasticsearch-claims-relative",
        false,
        "path.data: storage\n",
    );
    let data = host.directory("home/storage");

    // Act & Assert
    assert_eq!(host.claimed_trees(), [data.display().to_string()]);
}

#[test]
fn filesystem_claims_make_no_claim_for_a_directory_the_host_does_not_have() {
    // Arrange: a path nothing on the host answers to is not one to seal on a guess.
    let host = Box_::host_node("elasticsearch-claims-missing", false, "");
    let missing = host.path("not-created");
    write(
        &host.scratch,
        "conf/elasticsearch.yml",
        &data_setting(&[&missing]),
    );

    // Act & Assert
    assert!(host.claimed_trees().is_empty());
}

#[test]
fn filesystem_claims_leave_a_container_nodes_data_alone() {
    // Arrange: a root of the node's own, holding the same paths as the host's and different
    // directories at them, which is what a container's image is.
    let host = Box_::host_node("elasticsearch-claims-container", true, "");
    let data = host.directory("var-lib-elasticsearch");
    let setting = data_setting(&[&data]);
    let image = host.directory("image");
    fs::remove_file(host.proc.join("600/root")).expect("the fixture's link");
    symlink(&image, host.proc.join("600/root")).expect("a writable fixture");
    let inside = |path: &Path| image.join(path.strip_prefix("/").expect("an absolute path"));
    fs::create_dir_all(inside(&data)).expect("a writable fixture");
    let config = inside(&host.path("conf"));
    fs::create_dir_all(&config).expect("a writable fixture");
    fs::write(config.join("elasticsearch.yml"), setting).expect("a writable fixture");

    // Act & Assert
    assert!(host.claimed_trees().is_empty());
}

#[test]
fn filesystem_claims_make_no_claim_where_the_settings_cannot_be_read() {
    // Arrange: the walk's own default is the safe direction to be wrong in.
    let host = Box_::host_node(
        "elasticsearch-claims-unread",
        false,
        "path.data: ${ES_DATA}\n",
    );

    // Act & Assert
    assert!(host.claimed_trees().is_empty());
}

#[test]
fn filesystem_claims_seal_the_directory_a_symlinked_data_path_leads_to() {
    // Arrange: found by review. `/var/lib/elasticsearch` linking to `/mnt/es` passed the identity
    // check, and the claim named the link, which the walk matches as text: it would have walked
    // into `/mnt/es`, the live store itself.
    let host = Box_::host_node("elasticsearch-claims-symlinked-data", false, "");
    let real = host.directory("mnt-es");
    let link = host.path("var-lib-elasticsearch");
    symlink(&real, &link).expect("a writable fixture");
    write(
        &host.scratch,
        "conf/elasticsearch.yml",
        &data_setting(&[&link]),
    );

    // Act & Assert
    let canonical = fs::canonicalize(&real).expect("the real directory");
    assert_eq!(host.claimed_trees(), [canonical.display().to_string()]);
}

#[test]
fn filesystem_claims_seal_a_data_path_spelled_with_dot_dot_as_the_directory_it_is() {
    // Arrange
    let host = Box_::host_node("elasticsearch-claims-dot-dot", false, "");
    let real = host.directory("data");
    host.directory("home");
    let spelled = host.path("home/../data");
    write(
        &host.scratch,
        "conf/elasticsearch.yml",
        &data_setting(&[&spelled]),
    );

    // Act & Assert
    let canonical = fs::canonicalize(&real).expect("the real directory");
    assert_eq!(host.claimed_trees(), [canonical.display().to_string()]);
}

impl Box_ {
    /// The node's process running as another account than the one that owns the fixture.
    fn run_by_another_account(&self) {
        let owner = fs::metadata(&self.scratch).expect("the fixture");
        let (uid, gid) = (owner.uid() + 1, owner.gid() + 1);
        write(
            &self.proc,
            "600/status",
            &format!(
                "Uid:\t{uid}\t{uid}\t{uid}\t{uid}\nGid:\t{gid}\t{gid}\t{gid}\t{gid}\nGroups:\t{gid}\n"
            ),
        );
    }
}

#[test]
fn filesystem_claims_make_no_claim_for_a_directory_the_node_cannot_write() {
    // Arrange: found by the security review. Any account can start a process that reads as a
    // node and name `/etc` its data path, and a claim on it hid `/etc` from the walk. A node
    // writes its data and logs, so a directory its account cannot write is not its store.
    let host = Box_::host_node("elasticsearch-claims-not-writable", false, "");
    let data = host.directory("var-lib-elasticsearch");
    write(
        &host.scratch,
        "conf/elasticsearch.yml",
        &data_setting(&[&data]),
    );
    host.run_by_another_account();

    // Act & Assert
    assert!(host.claimed_trees().is_empty());
}

#[test]
fn filesystem_claims_make_no_claim_for_a_directory_only_the_world_can_write() {
    // Arrange: `/tmp` is writable by every account, the node's among them, and is not a store.
    let host = Box_::host_node("elasticsearch-claims-world-writable", false, "");
    let data = host.directory("tmp");
    fs::set_permissions(&data, fs::Permissions::from_mode(0o1777)).expect("a writable fixture");
    write(
        &host.scratch,
        "conf/elasticsearch.yml",
        &data_setting(&[&data]),
    );
    host.run_by_another_account();

    // Act & Assert
    assert!(host.claimed_trees().is_empty());
}

/// A container node: a root of its own, its data on a volume whose host directory is `volume`.
///
/// The node's `mountinfo` says the data path is a mount of device 254:1 at `root` inside that
/// device, and the host's says device 254:1 is mounted at `/` with root `/`, which is how the
/// kernel publishes a named volume or a bind mount. A mount point holding a space is spelled
/// `\040`, as the kernel writes it.
fn container_node_with_a_volume(name: &str, data: &str, volume: &Path) -> Box_ {
    let host = Box_::host_node(name, true, "");
    let image = host.directory("image");
    fs::remove_file(host.proc.join("600/root")).expect("the fixture's link");
    symlink(&image, host.proc.join("600/root")).expect("a writable fixture");
    let config = image.join(host.path("conf").strip_prefix("/").expect("absolute"));
    fs::create_dir_all(&config).expect("a writable fixture");
    fs::write(
        config.join("elasticsearch.yml"),
        format!("path.data: {data}\n"),
    )
    .expect("a fixture");

    let escaped = |path: &str| path.replace(' ', "\\040");
    write(
        &host.proc,
        "600/mountinfo",
        &format!(
            "21 1 0:99 / / rw - overlay overlay rw\n\
             22 21 254:1 {root} {point} rw - ext4 /dev/vda1 rw\n",
            root = escaped(&volume.display().to_string()),
            point = escaped(data),
        ),
    );
    write(
        &host.proc,
        "self/mountinfo",
        "1 0 254:1 / / rw - ext4 /dev/vda1 rw\n2 1 0:5 / /proc rw - proc proc rw\n",
    );
    host
}

#[test]
fn filesystem_claims_seal_the_host_directory_behind_a_container_nodes_volume() {
    // Arrange: found by the second domain review, measured: a node in a container with its data
    // on a named volume was read fine and 41 entries under the volume were walked, since the
    // in-container path is not a host path at all. The kernel says which host directory it is.
    let volume = Box_::host_node("elasticsearch-claims-volume-scratch", false, "")
        .directory("volumes/es-data/_data");
    let host = container_node_with_a_volume(
        "elasticsearch-claims-volume",
        "/usr/share/elasticsearch/data",
        &volume,
    );

    // Act & Assert
    let canonical = fs::canonicalize(&volume).expect("the volume");
    assert_eq!(host.claimed_trees(), [canonical.display().to_string()]);
}

#[test]
fn filesystem_claims_make_no_claim_for_a_volume_path_that_climbs_out_of_the_volume() {
    // Arrange: found by the security review. The mount match compared paths as text, so
    // `<volume>/../secret` matched the volume's mount, and the host resolved the `..` itself.
    let volume = Box_::host_node("elasticsearch-claims-volume-climb-scratch", false, "")
        .directory("volumes/es-data/_data");
    fs::create_dir_all(volume.join("../secret")).expect("a writable fixture");
    let host = container_node_with_a_volume(
        "elasticsearch-claims-volume-climb",
        "/usr/share/elasticsearch/data",
        &volume,
    );
    let config = host
        .path("image")
        .join(host.path("conf").strip_prefix("/").expect("absolute"));
    fs::write(
        config.join("elasticsearch.yml"),
        "path.data: /usr/share/elasticsearch/data/../secret\n",
    )
    .expect("a fixture");

    // Act & Assert
    assert!(host.claimed_trees().is_empty());
}

#[test]
fn filesystem_claims_read_a_mount_point_holding_a_space() {
    // Arrange
    let volume = Box_::host_node("elasticsearch-claims-volume-space-scratch", false, "")
        .directory("volumes/es data");
    let host = container_node_with_a_volume(
        "elasticsearch-claims-volume-space",
        "/usr/share/elasticsearch/my data",
        &volume,
    );

    // Act & Assert
    let canonical = fs::canonicalize(&volume).expect("the volume");
    assert_eq!(host.claimed_trees(), [canonical.display().to_string()]);
}

#[test]
fn filesystem_claims_seal_the_log_directory_beside_the_data() {
    // Arrange: measured by the second domain review, `gc.log` under an archive node's `logs`
    // moved between two runs 100 s apart on an idle box.
    let host = Box_::host_node("elasticsearch-claims-logs", false, "");
    let data = host.directory("home/data");
    let logs = host.directory("home/logs");

    // Act
    let mut claimed = host.claimed_trees();
    claimed.sort();

    // Assert
    assert_eq!(
        claimed,
        [data.display().to_string(), logs.display().to_string()]
    );
}

#[test]
fn filesystem_claims_name_the_node_they_were_made_for() {
    // Arrange: two nodes pointed at one directory is the state worth reading, so each claim
    // says which node, by the config directory that leads to it in `nodes`.
    let host = Box_::host_node("elasticsearch-claims-qualified", false, "");
    host.directory("home/data");

    // Act
    let qualifiers: Vec<String> =
        ElasticsearchCollector::reading(&host.proc, false, HttpClient::new())
            .filesystem_claims()
            .iter()
            .map(|claim| {
                claim
                    .qualifier()
                    .map(|qualifier| qualifier.as_str().to_owned())
                    .unwrap_or_default()
            })
            .collect();

    // Assert
    assert_eq!(qualifiers, [host.path("conf").display().to_string()]);
}

impl Box_ {
    /// The node's install holding `release`'s server jar.
    fn installed(&self, release: &str) {
        write(
            &self.scratch,
            &format!("home/lib/elasticsearch-{release}.jar"),
            "",
        );
    }

    /// The node holding `target` open for writing, as its descriptor `number`, with the flags a
    /// node's `node.lock` and `gc.log` carry, measured on 8.19.22: `O_WRONLY`.
    fn holding(&self, number: u32, target: &Path) {
        self.holding_with(number, target, "0400001");
    }

    /// The same, with the `flags` its `fdinfo` gives, in octal.
    fn holding_with(&self, number: u32, target: &Path, flags: &str) {
        fs::create_dir_all(self.proc.join("600/fd")).expect("a writable fixture");
        symlink(target, self.proc.join(format!("600/fd/{number}"))).expect("a writable fixture");
        write(
            &self.proc,
            &format!("600/fdinfo/{number}"),
            &format!("pos:\t0\nflags:\t{flags}\nmnt_id:\t30\n"),
        );
    }
}

#[test]
fn filesystem_claims_seal_the_store_the_node_holds_open_over_what_its_settings_say() {
    // Arrange: found by review. A node on 8.x or 9.x started with `-d -E path.data=/srv/es` takes
    // that setting away with its launcher, and its file names the default. The node itself holds
    // `node.lock` in each data directory and its server log in its logs directory, measured on
    // every cell of the matrix, so what it has open is where it keeps them.
    let host = Box_::host_node("elasticsearch-claims-held", false, "");
    host.directory("home/data");
    host.directory("home/logs");
    let data = host.directory("srv-es");
    let logs = host.directory("srv-logs");
    host.holding(5, &data.join("node.lock"));
    host.holding(6, &logs.join("search_server.json"));

    // Act
    let mut claimed = host.claimed_trees();
    claimed.sort();

    // Assert
    assert_eq!(
        claimed,
        [data.display().to_string(), logs.display().to_string()]
    );
}

#[test]
fn filesystem_claims_take_the_logs_from_the_settings_where_the_node_holds_no_log_open() {
    // Arrange: found by review. A node whose Log4j appender has another name and whose GC log is
    // off holds `node.lock` and no log, and its held data alone left the logs directory unsealed.
    let host = Box_::host_node("elasticsearch-claims-held-no-log", false, "");
    let logs = host.directory("home/logs");
    let data = host.directory("srv-es");
    host.holding(5, &data.join("node.lock"));

    // Act
    let mut claimed = host.claimed_trees();
    claimed.sort();

    // Assert
    assert_eq!(
        claimed,
        [logs.display().to_string(), data.display().to_string()]
    );
}

#[test]
fn filesystem_claims_seal_a_7_nodes_store_above_its_nodes_directory() {
    // Arrange: measured on 7.17 and 6.8, the lock is under `<path.data>/nodes/0/`, and one data
    // path each holds one; the JVM's own `gc.log` marks the logs directory where no server log
    // is kept, as in a container that logs to stdout.
    let host = Box_::host_node("elasticsearch-claims-held-7", false, "");
    host.installed("7.17.29");
    let first = host.directory("data-a");
    let second = host.directory("data-b");
    let logs = host.directory("jvm-logs");
    host.directory("data-a/nodes/0");
    host.directory("data-b/nodes/0");
    host.holding(5, &first.join("nodes/0/node.lock"));
    host.holding(6, &second.join("nodes/0/node.lock"));
    host.holding(7, &logs.join("gc.log"));

    // Act
    let mut claimed = host.claimed_trees();
    claimed.sort();

    // Assert
    assert_eq!(
        claimed,
        [
            first.display().to_string(),
            second.display().to_string(),
            logs.display().to_string()
        ]
    );
}

#[test]
fn filesystem_claims_take_no_directory_from_an_unrelated_open_log() {
    // Arrange: measured on cell 07, a node started from a shell held the shell's `/tmp/run.log`
    // as its stdout, which is no directory of the node's.
    let host = Box_::host_node("elasticsearch-claims-held-stray", false, "");
    let data = host.directory("srv-es");
    let elsewhere = host.directory("tmp");
    host.holding(5, &data.join("node.lock"));
    host.holding(1, &elsewhere.join("run.log"));

    // Act
    let claimed = host.claimed_trees();

    // Assert
    assert_eq!(claimed, [data.display().to_string()]);
}

#[test]
fn filesystem_claims_take_an_8_nodes_lock_directory_as_its_data_whatever_it_is_called() {
    // Arrange: found by review. On 8.x and 9.x the lock is `<path.data>/node.lock`, so a data path
    // that happens to be `…/nodes/0` is that directory, and reading it as 7.x's
    // `<path.data>/nodes/<ordinal>` sealed the tree two levels above it instead.
    let host = Box_::host_node("elasticsearch-claims-held-8-nodes", false, "");
    host.installed("9.5.4");
    let data = host.directory("srv/nodes/0");
    host.holding(5, &data.join("node.lock"));

    // Act
    let claimed = host.claimed_trees();

    // Assert
    assert_eq!(claimed, [data.display().to_string()]);
}

#[test]
fn filesystem_claims_climb_a_7_nodes_ordinal_only_where_it_is_a_number() {
    // Arrange: 7.x names the ordinal directory `0`, `1`, …, and nothing else.
    let host = Box_::host_node("elasticsearch-claims-held-7-named", false, "");
    host.installed("7.17.29");
    let data = host.directory("srv/nodes/current");
    host.holding(5, &data.join("node.lock"));

    // Act
    let claimed = host.claimed_trees();

    // Assert
    assert_eq!(claimed, [data.display().to_string()]);
}

#[test]
fn filesystem_claims_seal_a_store_the_node_holds_open_for_writing_whatever_its_mode_bits_say() {
    // Arrange: found by review. A root-owned directory that a POSIX ACL lets the node's account
    // write fails a check of the mode bits, and the live store was walked. A file the node holds
    // open for writing there is the kernel's own word that it may, ACLs and capabilities included.
    let host = Box_::host_node("elasticsearch-claims-held-acl", false, "");
    let data = host.directory("srv-es");
    host.holding(5, &data.join("node.lock"));
    host.run_by_another_account();

    // Act & Assert
    assert_eq!(host.claimed_trees(), [data.display().to_string()]);
}

#[test]
fn filesystem_claims_take_no_store_from_a_file_held_open_only_for_reading() {
    // Arrange: reading a file proves nothing about writing beside it, and any account can open a
    // file named `node.lock` it can read.
    let host = Box_::host_node("elasticsearch-claims-held-read-only", false, "");
    let elsewhere = host.directory("elsewhere");
    host.holding_with(5, &elsewhere.join("node.lock"), "0400000");

    // Act & Assert
    assert!(
        !host
            .claimed_trees()
            .contains(&elsewhere.display().to_string())
    );
}

#[test]
fn filesystem_claims_make_no_claim_for_a_world_writable_directory_a_node_holds_a_log_in() {
    // Arrange: any account can create `x_server.json` in `/tmp` and hold it open for writing.
    let host = Box_::host_node("elasticsearch-claims-held-world", false, "");
    let shared = host.directory("tmp");
    fs::set_permissions(&shared, fs::Permissions::from_mode(0o1777)).expect("a writable fixture");
    let data = host.directory("srv-es");
    host.holding(5, &data.join("node.lock"));
    host.holding(6, &shared.join("x_server.json"));

    // Act & Assert
    assert_eq!(host.claimed_trees(), [data.display().to_string()]);
}
