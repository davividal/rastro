//! What a node was told at start, read from a `/proc` a test built.
//!
//! These are the settings the dispatch needs before it may ask the node anything: which port
//! serves HTTP, and whether that port wants TLS. The node's own answer comes later and is
//! authoritative; this read exists because asking first would be asking blind.

use std::fs;

use rastro::collectors::elasticsearch::{NodeSettings, ResidentNode};

mod support;

use support::fs_tree::{scratch_tree, write};

/// A 7.x server from the docker image, which carries its own paths, and whose environment holds
/// settings because it is the docker distribution.
const SERVER_ARGV: &str = "/usr/share/elasticsearch/jdk/bin/java\0\
    -Des.path.home=/usr/share/elasticsearch\0-Des.path.conf=/etc/elasticsearch\0\
    -Des.distribution.type=docker\0\
    -cp\0/usr/share/elasticsearch/lib/*\0org.elasticsearch.bootstrap.Elasticsearch\0";

/// The same server from a tarball, whose environment holds no settings at all.
const TAR_SERVER_ARGV: &str = "/usr/share/elasticsearch/jdk/bin/java\0\
    -Des.path.home=/usr/share/elasticsearch\0-Des.path.conf=/etc/elasticsearch\0\
    -Des.distribution.type=tar\0\
    -cp\0/usr/share/elasticsearch/lib/*\0org.elasticsearch.bootstrap.Elasticsearch\0";

/// The node's own file, under its root, because a node in a container reads the one in its
/// image rather than the host's.
const CONFIG_FILE: &str = "600/root/etc/elasticsearch/elasticsearch.yml";

fn node_in(proc: &std::path::Path) -> ResidentNode {
    ResidentNode::all_in(proc)
        .into_iter()
        .next()
        .expect("the fixture holds one server")
}

#[test]
fn read_in_flattens_nested_and_dotted_keys_alike() {
    // Arrange: the same file may spell settings either way, and ES reads both.
    let proc = scratch_tree("elasticsearch-settings-flatten", &["600/root"]);
    write(&proc, "600/cmdline", SERVER_ARGV);
    write(&proc, "600/environ", "");
    write(
        &proc,
        CONFIG_FILE,
        "http:\n  port: 9201\nxpack.security.enabled: false\n",
    );

    // Act
    let settings = NodeSettings::read_in(&proc, &node_in(&proc)).expect("readable settings");

    // Assert
    assert_eq!(settings.get("http.port"), Some("9201"));
    assert_eq!(settings.get("xpack.security.enabled"), Some("false"));
}

#[test]
fn read_in_takes_a_dotted_environment_variable_over_the_file_on_the_docker_distribution() {
    // Arrange: the docker image hands settings over as variables named after the setting, and
    // from 8.x they appear nowhere in the argv.
    let proc = scratch_tree("elasticsearch-settings-environ", &["600/root"]);
    write(&proc, "600/cmdline", SERVER_ARGV);
    write(
        &proc,
        "600/environ",
        "PATH=/usr/bin\0http.port=9300\0ES_JAVA_OPTS=-Xms512m\0",
    );
    write(&proc, CONFIG_FILE, "http.port: 9201\n");

    // Act
    let settings = NodeSettings::read_in(&proc, &node_in(&proc)).expect("readable settings");

    // Assert
    assert_eq!(settings.get("http.port"), Some("9300"));
    assert_eq!(settings.get("PATH"), None);
}

#[test]
fn read_in_takes_the_environment_over_a_command_line_setting_on_the_docker_distribution() {
    // Arrange: measured by the domain review on the 7.17.24, 8.15.3 and 9.2.0 images, with all
    // three sources set: the node ran on the environment's value. On 7.17 the entrypoint appends
    // the variables as `-E` flags after the command's own, and the last flag wins.
    let proc = scratch_tree("elasticsearch-settings-argv", &["600/root"]);
    let argv = format!("{SERVER_ARGV}-Ehttp.port=9400\0");
    write(&proc, "600/cmdline", &argv);
    write(&proc, "600/environ", "http.port=9300\0");
    write(&proc, CONFIG_FILE, "http.port: 9201\n");

    // Act
    let settings = NodeSettings::read_in(&proc, &node_in(&proc)).expect("readable settings");

    // Assert
    assert_eq!(settings.get("http.port"), Some("9300"));
}

#[test]
fn read_in_takes_no_setting_from_the_environment_of_a_tarball_install() {
    // Arrange: measured on the 7.17.24 and 8.15.3 tarballs, a dotted variable is not a setting
    // outside the docker distribution: the node ran on its file's value with one set.
    let proc = scratch_tree("elasticsearch-settings-tar-environ", &["600/root"]);
    write(&proc, "600/cmdline", TAR_SERVER_ARGV);
    write(&proc, "600/environ", "http.port=9300\0");
    write(&proc, CONFIG_FILE, "http.port: 9201\n");

    // Act
    let settings = NodeSettings::read_in(&proc, &node_in(&proc)).expect("readable settings");

    // Assert
    assert_eq!(settings.get("http.port"), Some("9201"));
}

#[test]
fn read_in_takes_a_command_line_setting_over_the_file_of_a_tarball_install() {
    // Arrange: measured on the same tarballs, `-E` over the file, and over an environment
    // variable the node does not read at all.
    let proc = scratch_tree("elasticsearch-settings-tar-argv", &["600/root"]);
    let argv = format!("{TAR_SERVER_ARGV}-Ehttp.port=9400\0");
    write(&proc, "600/cmdline", &argv);
    write(&proc, "600/environ", "http.port=9300\0");
    write(&proc, CONFIG_FILE, "http.port: 9201\n");

    // Act
    let settings = NodeSettings::read_in(&proc, &node_in(&proc)).expect("readable settings");

    // Assert
    assert_eq!(settings.get("http.port"), Some("9400"));
}

#[test]
fn read_in_refuses_a_node_that_names_no_distribution() {
    // Arrange: without it, whether the environment holds settings cannot be told, and the two
    // answers can disagree on the very port or protocol the node is asked on.
    let proc = scratch_tree("elasticsearch-settings-no-distribution", &["600/root"]);
    write(
        &proc,
        "600/cmdline",
        "/usr/share/elasticsearch/jdk/bin/java\0-Des.path.conf=/etc/elasticsearch\0\
         -cp\0/usr/share/elasticsearch/lib/*\0org.elasticsearch.bootstrap.Elasticsearch\0",
    );
    write(&proc, "600/environ", "");

    // Act
    let unread = NodeSettings::read_in(&proc, &node_in(&proc)).expect_err("no distribution");

    // Assert
    assert!(
        unread.reason().contains("es.distribution.type"),
        "{}",
        unread.reason()
    );
}

#[test]
fn read_in_takes_an_8_nodes_command_line_settings_from_its_launcher() {
    // Arrange: measured on 8.15.3, the server's argv carries no `-E`; the launcher's does.
    let proc = scratch_tree("elasticsearch-settings-launcher", &["600/root", "40"]);
    write(
        &proc,
        "40/cmdline",
        "/usr/share/elasticsearch/jdk/bin/java\0-Des.path.home=/usr/share/elasticsearch\0\
         -Des.path.conf=/etc/elasticsearch\0-Des.distribution.type=tar\0\
         -cp\0/usr/share/elasticsearch/lib/*\0\
         org.elasticsearch.launcher.CliToolLauncher\0-Ehttp.port=9400\0",
    );
    write(
        &proc,
        "600/cmdline",
        "/usr/share/elasticsearch/jdk/bin/java\0-m\0\
         org.elasticsearch.server/org.elasticsearch.bootstrap.Elasticsearch\0",
    );
    write(
        &proc,
        "600/stat",
        "600 (java) S 40 600 1 0 -1 4194560 0 0 0 0\n",
    );
    write(&proc, "600/environ", "http.port=9300\0");
    write(&proc, CONFIG_FILE, "http.port: 9201\n");

    // Act
    let settings = NodeSettings::read_in(&proc, &node_in(&proc)).expect("readable settings");

    // Assert
    assert_eq!(settings.get("http.port"), Some("9400"));
}

#[test]
fn read_in_joins_a_list_with_commas_as_elasticsearch_reads_one() {
    // Arrange
    let proc = scratch_tree("elasticsearch-settings-list", &["600/root"]);
    write(&proc, "600/cmdline", SERVER_ARGV);
    write(&proc, "600/environ", "");
    write(&proc, CONFIG_FILE, "network.host: [_local_, _site_]\n");

    // Act
    let settings = NodeSettings::read_in(&proc, &node_in(&proc)).expect("readable settings");

    // Assert
    assert_eq!(settings.get("network.host"), Some("_local_,_site_"));
}

#[test]
fn read_in_substitutes_a_variable_from_the_nodes_own_environment() {
    // Arrange: `${NAME}` in the file is resolved by the node from its environment at start.
    let proc = scratch_tree("elasticsearch-settings-substitution", &["600/root"]);
    write(&proc, "600/cmdline", SERVER_ARGV);
    write(&proc, "600/environ", "ES_HTTP_PORT=9250\0");
    write(&proc, CONFIG_FILE, "http.port: ${ES_HTTP_PORT}\n");

    // Act
    let settings = NodeSettings::read_in(&proc, &node_in(&proc)).expect("readable settings");

    // Assert
    assert_eq!(settings.get("http.port"), Some("9250"));
}

#[test]
fn read_in_refuses_a_variable_the_environment_does_not_hold() {
    // Arrange
    let proc = scratch_tree("elasticsearch-settings-unresolved", &["600/root"]);
    write(&proc, "600/cmdline", SERVER_ARGV);
    write(&proc, "600/environ", "");
    write(&proc, CONFIG_FILE, "http.port: ${ES_HTTP_PORT}\n");

    // Act
    let unread = NodeSettings::read_in(&proc, &node_in(&proc)).expect_err("an unresolvable value");

    // Assert: a port rastro cannot resolve is not a port it may dial.
    assert!(
        unread.reason().contains("ES_HTTP_PORT"),
        "{}",
        unread.reason()
    );
}

#[test]
fn read_in_reads_no_file_where_the_config_directory_holds_none() {
    // Arrange
    let proc = scratch_tree("elasticsearch-settings-no-file", &["600/root"]);
    write(&proc, "600/cmdline", SERVER_ARGV);
    write(&proc, "600/environ", "http.port=9300\0");

    // Act
    let settings = NodeSettings::read_in(&proc, &node_in(&proc)).expect("readable settings");

    // Assert
    assert_eq!(settings.get("http.port"), Some("9300"));
}

#[test]
fn read_in_refuses_a_node_whose_config_directory_is_not_in_its_argv() {
    // Arrange: a server started by hand, whose file rastro cannot locate without guessing.
    let proc = scratch_tree("elasticsearch-settings-no-conf", &["600/root"]);
    write(
        &proc,
        "600/cmdline",
        "java\0-cp\0lib/*\0org.elasticsearch.bootstrap.Elasticsearch\0",
    );
    write(&proc, "600/environ", "");

    // Act
    let unread = NodeSettings::read_in(&proc, &node_in(&proc)).expect_err("no config directory");

    // Assert
    assert!(
        unread.reason().contains("es.path.conf"),
        "{}",
        unread.reason()
    );
}

#[test]
fn read_in_refuses_an_environment_it_cannot_read() {
    // Arrange: a directory where the file should be fails for root as well, where a mode would
    // not, and the unprivileged refusal this stands for is the ordinary case for this read.
    let proc = scratch_tree(
        "elasticsearch-settings-environ-refused",
        &["600/root", "600/environ"],
    );
    write(&proc, "600/cmdline", SERVER_ARGV);

    // Act
    let unread = NodeSettings::read_in(&proc, &node_in(&proc)).expect_err("an unreadable environ");

    // Assert
    assert!(unread.reason().contains("environ"), "{}", unread.reason());
}

#[test]
fn read_in_refuses_a_file_that_is_not_yaml() {
    // Arrange
    let proc = scratch_tree("elasticsearch-settings-malformed", &["600/root"]);
    write(&proc, "600/cmdline", SERVER_ARGV);
    write(&proc, "600/environ", "");
    write(&proc, CONFIG_FILE, "http:\n  port: [9201\n");

    // Act
    let unread = NodeSettings::read_in(&proc, &node_in(&proc)).expect_err("malformed YAML");

    // Assert
    assert!(
        unread.reason().contains("elasticsearch.yml"),
        "{}",
        unread.reason()
    );
}

#[test]
fn read_in_refuses_a_file_it_cannot_read() {
    // Arrange: again a directory, so the refusal holds when the suite runs as root.
    let proc = scratch_tree(
        "elasticsearch-settings-file-refused",
        &["600/root/etc/elasticsearch/elasticsearch.yml"],
    );
    write(&proc, "600/cmdline", SERVER_ARGV);
    write(&proc, "600/environ", "");
    assert!(
        fs::metadata(proc.join(CONFIG_FILE))
            .expect("the fixture")
            .is_dir()
    );

    // Act
    let unread = NodeSettings::read_in(&proc, &node_in(&proc)).expect_err("an unreadable file");

    // Assert
    assert!(
        unread.reason().contains("elasticsearch.yml"),
        "{}",
        unread.reason()
    );
}

#[test]
fn read_in_refuses_a_node_whose_argv_cannot_be_read_exactly() {
    // Arrange: a setting or a path spelled in bytes that are not UTF-8 cannot be read back as the
    // node reads it, and a near copy is a wrong port or a wrong file.
    let proc = scratch_tree("elasticsearch-settings-latin1", &["600/root"]);
    let mut argv =
        b"/usr/share/elasticsearch/jdk/bin/java\0-Des.path.conf=/etc/elasticsearch\0".to_vec();
    argv.extend_from_slice(
        b"-cp\0/opt/lib/*\0org.elasticsearch.bootstrap.Elasticsearch\0-Enode.name=n\xe9\0",
    );
    std::fs::write(proc.join("600/cmdline"), argv).expect("a writable fixture");
    write(&proc, "600/environ", "");

    // Act
    let unread = NodeSettings::read_in(&proc, &node_in(&proc)).expect_err("an inexact argv");

    // Assert
    assert!(unread.reason().contains("UTF-8"), "{}", unread.reason());
}

#[test]
fn read_in_reads_past_a_variable_that_is_not_utf_8_and_is_not_a_setting() {
    // Arrange: one Latin-1 value anywhere in the environment used to make the whole node
    // unread, over a variable the collector never looks at.
    let proc = scratch_tree("elasticsearch-settings-environ-latin1", &["600/root"]);
    write(&proc, "600/cmdline", SERVER_ARGV);
    std::fs::write(
        proc.join("600/environ"),
        b"LANG_NOTE=caf\xe9\0http.port=9300\0",
    )
    .expect("a writable fixture");

    // Act
    let settings = NodeSettings::read_in(&proc, &node_in(&proc)).expect("readable settings");

    // Assert
    assert_eq!(settings.get("http.port"), Some("9300"));
}

#[test]
fn read_in_refuses_a_setting_in_the_environment_that_is_not_utf_8() {
    // Arrange: a setting rastro cannot read exactly is not one it may act on.
    let proc = scratch_tree(
        "elasticsearch-settings-environ-setting-latin1",
        &["600/root"],
    );
    write(&proc, "600/cmdline", SERVER_ARGV);
    std::fs::write(proc.join("600/environ"), b"node.name=n\xe9\0").expect("a writable fixture");

    // Act
    let unread = NodeSettings::read_in(&proc, &node_in(&proc)).expect_err("an unreadable setting");

    // Assert
    assert!(unread.reason().contains("node.name"), "{}", unread.reason());
}
