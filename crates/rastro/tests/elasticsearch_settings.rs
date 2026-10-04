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
    if !proc.join("600/stat").exists() {
        support::process::started(proc, "600", "1", 5);
    }
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
        "/usr/share/elasticsearch/jdk/bin/java\0-Des.distribution.type=tar\0\
         --module-path\0/usr/share/elasticsearch/lib\0\
         -m\0org.elasticsearch.server/org.elasticsearch.bootstrap.Elasticsearch\0",
    );
    support::process::started(&proc, "600", "40", 5);
    write(
        &proc,
        "600/environ",
        "ES_PATH_CONF=/etc/elasticsearch\0http.port=9300\0",
    );
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
fn read_in_reads_a_node_that_started_with_no_file() {
    // Arrange: measured by the second domain review, an 8.15.3 node starts and serves with no
    // `elasticsearch.yml`, configured by `-E` and its environment alone, which is the usual shape
    // in orchestrators. Its config directory has not changed since it started.
    let proc = scratch_tree(
        "elasticsearch-settings-no-file",
        &["600/root/etc/elasticsearch"],
    );
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

    write(&proc, CONFIG_FILE, "");

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

#[cfg(target_os = "linux")]
#[test]
fn read_in_resolves_an_absolute_symlink_inside_the_nodes_own_root() {
    // Arrange: measured in the podman VM, an absolute symlink met under `/proc/<pid>/root` resolves
    // against the reader's root, not the container's: it found nothing, or found the host's file
    // at that path. Here the link's target exists only inside the node's root, and says TLS, so
    // reading past the root would put the node on its defaults and send it plaintext.
    let proc = scratch_tree(
        "elasticsearch-settings-absolute-link",
        &["600/root/etc/elasticsearch", "600/root/srv/es-config"],
    );
    write(&proc, "600/cmdline", SERVER_ARGV);
    write(&proc, "600/environ", "");
    write(
        &proc,
        "600/root/srv/es-config/elasticsearch.yml",
        "xpack.security.http.ssl:\n  enabled: true\n",
    );
    std::os::unix::fs::symlink("/srv/es-config/elasticsearch.yml", proc.join(CONFIG_FILE))
        .expect("a writable fixture");

    // Act
    let settings = NodeSettings::read_in(&proc, &node_in(&proc)).expect("readable settings");

    // Assert
    assert_eq!(
        settings.get("xpack.security.http.ssl.enabled"),
        Some("true")
    );
}

#[cfg(target_os = "linux")]
#[test]
fn read_in_keeps_a_relative_symlink_that_climbs_out_inside_the_nodes_own_root() {
    // Arrange: `..` past the top of the root stays at the top, as it does for the node itself.
    let proc = scratch_tree(
        "elasticsearch-settings-climbing-link",
        &["600/root/etc/elasticsearch", "600/root/srv/es-config"],
    );
    write(&proc, "600/cmdline", SERVER_ARGV);
    write(&proc, "600/environ", "");
    write(
        &proc,
        "600/root/srv/es-config/elasticsearch.yml",
        "http.port: 9250\n",
    );
    std::os::unix::fs::symlink(
        "../../../../../../../srv/es-config/elasticsearch.yml",
        proc.join(CONFIG_FILE),
    )
    .expect("a writable fixture");

    // Act
    let settings = NodeSettings::read_in(&proc, &node_in(&proc)).expect("readable settings");

    // Assert
    assert_eq!(settings.get("http.port"), Some("9250"));
}

#[test]
fn read_in_takes_a_placeholders_default_where_its_variable_is_unset() {
    // Arrange: measured on 8.15.3, `node.name: ${ES_UNSET_NAME:from-default}` with the variable
    // unset started a node named `from-default`. Found by review: this read took the whole of
    // `ES_HTTP_PORT:9250` for a variable name and refused the node.
    let proc = scratch_tree("elasticsearch-settings-placeholder-default", &["600/root"]);
    write(&proc, "600/cmdline", SERVER_ARGV);
    write(&proc, "600/environ", "");
    write(&proc, CONFIG_FILE, "http.port: ${ES_HTTP_PORT:9250}\n");

    // Act
    let settings = NodeSettings::read_in(&proc, &node_in(&proc)).expect("readable settings");

    // Assert
    assert_eq!(settings.get("http.port"), Some("9250"));
}

#[test]
fn read_in_takes_a_placeholders_variable_over_its_default() {
    // Arrange
    let proc = scratch_tree("elasticsearch-settings-placeholder-set", &["600/root"]);
    write(&proc, "600/cmdline", SERVER_ARGV);
    write(&proc, "600/environ", "ES_HTTP_PORT=9260\0");
    write(&proc, CONFIG_FILE, "http.port: ${ES_HTTP_PORT:9250}\n");

    // Act
    let settings = NodeSettings::read_in(&proc, &node_in(&proc)).expect("readable settings");

    // Assert
    assert_eq!(settings.get("http.port"), Some("9260"));
}

#[test]
fn read_in_decodes_an_encoded_setting_on_the_docker_distribution() {
    // Arrange: measured on the 7.17.24 and 8.15.3 images, `ES_SETTING_NODE_ATTR_RACK__ID=r1`
    // became `node.attr.rack_id`: a single underscore is a dot, a doubled one an underscore. For
    // an orchestrator that cannot put dots in a variable's name. Found by review.
    let proc = scratch_tree("elasticsearch-settings-encoded", &["600/root"]);
    write(&proc, "600/cmdline", SERVER_ARGV);
    write(
        &proc,
        "600/environ",
        "ES_SETTING_NODE_ATTR_RACK__ID=r1\0ES_SETTING_HTTP_PORT=9300\0",
    );
    write(&proc, CONFIG_FILE, "http.port: 9201\n");

    // Act
    let settings = NodeSettings::read_in(&proc, &node_in(&proc)).expect("readable settings");

    // Assert
    assert_eq!(settings.get("node.attr.rack_id"), Some("r1"));
    assert_eq!(settings.get("http.port"), Some("9300"));
}

#[test]
fn read_in_refuses_an_encoded_and_a_dotted_setting_that_disagree() {
    // Arrange: nothing measured says which of the two the image applies, so rastro does not pick.
    let proc = scratch_tree("elasticsearch-settings-encoded-conflict", &["600/root"]);
    write(&proc, "600/cmdline", SERVER_ARGV);
    write(
        &proc,
        "600/environ",
        "ES_SETTING_HTTP_PORT=9300\0http.port=9400\0",
    );

    write(&proc, CONFIG_FILE, "");

    // Act
    let unread = NodeSettings::read_in(&proc, &node_in(&proc)).expect_err("a conflict");

    // Assert
    assert!(unread.reason().contains("http.port"), "{}", unread.reason());
}

#[test]
fn read_in_takes_no_encoded_setting_on_a_tarball_install() {
    // Arrange: the encoding is the docker image's, as the dotted form is.
    let proc = scratch_tree("elasticsearch-settings-encoded-tar", &["600/root"]);
    write(&proc, "600/cmdline", TAR_SERVER_ARGV);
    write(&proc, "600/environ", "ES_SETTING_HTTP_PORT=9300\0");
    write(&proc, CONFIG_FILE, "http.port: 9201\n");

    // Act
    let settings = NodeSettings::read_in(&proc, &node_in(&proc)).expect("readable settings");

    // Assert
    assert_eq!(settings.get("http.port"), Some("9201"));
}

#[test]
fn read_in_refuses_a_node_launched_with_an_argument_file() {
    // Arrange: the `java` launcher expands `@file` in place, so a property in it, a later
    // `es.path.conf` say, overrides what the argv shows, and rastro cannot read what the JVM did.
    let proc = scratch_tree("elasticsearch-settings-argument-file", &["600/root"]);
    write(
        &proc,
        "600/cmdline",
        "/usr/share/elasticsearch/jdk/bin/java\0-Des.path.conf=/etc/elasticsearch\0\
         -Des.distribution.type=tar\0@/etc/elasticsearch/jvm.args\0\
         -cp\0/usr/share/elasticsearch/lib/*\0org.elasticsearch.bootstrap.Elasticsearch\0",
    );
    write(&proc, "600/environ", "");

    // Act
    let unread = NodeSettings::read_in(&proc, &node_in(&proc)).expect_err("an argument file");

    // Assert
    assert!(
        unread.reason().contains("argument file"),
        "{}",
        unread.reason()
    );
}

#[test]
fn read_in_takes_a_command_line_setting_given_as_two_arguments() {
    // Arrange: found by review. Elasticsearch declares `-E` with a required argument, so
    // `-E http.port=9400` is as valid as `-Ehttp.port=9400`, and was ignored.
    let proc = scratch_tree("elasticsearch-settings-separated-e", &["600/root"]);
    let argv = format!("{TAR_SERVER_ARGV}-E\0http.port=9400\0-E\0xpack.security.enabled=true\0");
    write(&proc, "600/cmdline", &argv);
    write(&proc, "600/environ", "");
    write(&proc, CONFIG_FILE, "http.port: 9201\n");

    // Act
    let settings = NodeSettings::read_in(&proc, &node_in(&proc)).expect("readable settings");

    // Assert
    assert_eq!(settings.get("http.port"), Some("9400"));
    assert_eq!(settings.get("xpack.security.enabled"), Some("true"));
}

#[test]
fn read_in_reads_a_file_changed_after_the_node_started() {
    // Arrange: the file is what the node would start with now, staged changes and all. Nothing on
    // the box says what it read then, and only TLS on HTTP is decided from it before asking: a
    // file staging TLS off over a listener still on TLS is one more way into the blind spot
    // `docs/decisions.md` accepts, where the node answers in the wrong protocol and is an error.
    let proc = scratch_tree("elasticsearch-settings-staged", &["600/root"]);
    write(&proc, "600/cmdline", SERVER_ARGV);
    write(&proc, "600/environ", "");
    write(&proc, CONFIG_FILE, "http.port: 9201\n");
    support::process::started(&proc, "600", "1", 3600);

    // Act
    let settings = NodeSettings::read_in(&proc, &node_in(&proc)).expect("readable settings");

    // Assert
    assert_eq!(settings.get("http.port"), Some("9201"));
}

#[test]
fn read_in_reads_a_node_whose_start_cannot_be_read() {
    // Arrange: when the node started no longer decides anything.
    let proc = scratch_tree("elasticsearch-settings-no-start", &["600/root"]);
    write(&proc, "600/cmdline", SERVER_ARGV);
    write(&proc, "600/environ", "");
    write(&proc, CONFIG_FILE, "http.port: 9201\n");
    write(&proc, "600/stat", "600 (java) S\n");

    // Act
    let settings = NodeSettings::read_in(&proc, &node_in(&proc)).expect("readable settings");

    // Assert
    assert_eq!(settings.get("http.port"), Some("9201"));
}

#[test]
fn read_in_takes_a_command_line_setting_spelled_with_an_equals_sign() {
    // Arrange: found by the second domain review, measured on 8.15.3: jopt-simple also takes
    // `-E=name=value`, which was read as a setting named `""` and let a TLS node be sent plaintext.
    let proc = scratch_tree("elasticsearch-settings-e-equals", &["600/root"]);
    let argv = format!("{TAR_SERVER_ARGV}-E=xpack.security.http.ssl.enabled=true\0");
    write(&proc, "600/cmdline", &argv);
    write(&proc, "600/environ", "");
    write(&proc, CONFIG_FILE, "");

    // Act
    let settings = NodeSettings::read_in(&proc, &node_in(&proc)).expect("readable settings");

    // Assert
    assert_eq!(
        settings.transport(),
        rastro::collectors::elasticsearch::Transport::Tls
    );
}

#[test]
fn read_in_reads_past_the_servers_own_flags() {
    // Arrange: the server's options, from its own `--help` on 7.17.24, 8.15.3 and 9.2.0.
    let proc = scratch_tree("elasticsearch-settings-known-flags", &["600/root"]);
    let argv = format!(
        "{TAR_SERVER_ARGV}-d\0-p\0/var/run/es.pid\0--pidfile=/var/run/es.pid\0-q\0--silent\0-Ehttp.port=9400\0"
    );
    write(&proc, "600/cmdline", &argv);
    write(&proc, "600/environ", "");
    write(&proc, CONFIG_FILE, "");

    // Act
    let settings = NodeSettings::read_in(&proc, &node_in(&proc)).expect("readable settings");

    // Assert
    assert_eq!(settings.get("http.port"), Some("9400"));
}

#[test]
fn read_in_refuses_a_command_line_argument_it_cannot_place() {
    // Arrange: three rounds found three spellings of `-E` one at a time. A closed set of the
    // server's options, and a refusal for anything else, ends that: a spelling not yet known is
    // refused rather than misread.
    let proc = scratch_tree("elasticsearch-settings-unknown-argument", &["600/root"]);
    let argv = format!("{TAR_SERVER_ARGV}--Ehttp.port=9400\0");
    write(&proc, "600/cmdline", &argv);
    write(&proc, "600/environ", "");
    write(&proc, CONFIG_FILE, "");

    // Act
    let unread = NodeSettings::read_in(&proc, &node_in(&proc)).expect_err("an unplaced argument");

    // Assert
    assert!(
        unread.reason().contains("--Ehttp.port=9400"),
        "{}",
        unread.reason()
    );
}

#[test]
fn read_in_reads_a_node_whose_launcher_has_exited_from_its_file_and_environment() {
    // Arrange: measured on 8.19, 9.4 and 9.5 (cells 05 and 06), `bin/elasticsearch -d` returns
    // once the node is up and its launcher exits, taking the `-E` settings with it. The node is
    // still read: its file and its environment are on the box, and what only `-E` set is the
    // blind spot `docs/decisions.md` accepts.
    let proc = scratch_tree("elasticsearch-settings-daemonised", &["600/root", "1"]);
    write(&proc, "1/cmdline", "/sbin/init\0");
    write(
        &proc,
        "600/cmdline",
        "/usr/share/elasticsearch/jdk/bin/java\0-Des.distribution.type=tar\0\
         --module-path\0/usr/share/elasticsearch/lib\0\
         -m\0org.elasticsearch.server/org.elasticsearch.bootstrap.Elasticsearch\0",
    );
    write(&proc, "600/environ", "ES_PATH_CONF=/etc/elasticsearch\0");
    write(&proc, CONFIG_FILE, "http.port: 9201\n");
    support::process::started(&proc, "600", "1", 5);

    // Act
    let settings = NodeSettings::read_in(&proc, &node_in(&proc)).expect("readable settings");

    // Assert
    assert_eq!(settings.get("http.port"), Some("9201"));
}

#[test]
fn read_in_substitutes_a_placeholder_in_an_environment_setting() {
    // Arrange: measured by the second domain review on 8.15.3, `-e http.port='${HP:9250}'` bound
    // 9250: the node resolves placeholders after merging every source, not in its file alone.
    let proc = scratch_tree("elasticsearch-settings-placeholder-environ", &["600/root"]);
    write(&proc, "600/cmdline", SERVER_ARGV);
    write(&proc, "600/environ", "http.port=${HP:9250}\0");
    write(&proc, CONFIG_FILE, "");

    // Act
    let settings = NodeSettings::read_in(&proc, &node_in(&proc)).expect("readable settings");

    // Assert
    assert_eq!(settings.get("http.port"), Some("9250"));
}

#[test]
fn read_in_substitutes_a_placeholder_in_a_command_line_setting() {
    // Arrange
    let proc = scratch_tree("elasticsearch-settings-placeholder-argv", &["600/root"]);
    let argv = format!("{TAR_SERVER_ARGV}-Ehttp.port=${{HP}}\0");
    write(&proc, "600/cmdline", &argv);
    write(&proc, "600/environ", "HP=9300\0");
    write(&proc, CONFIG_FILE, "");

    // Act
    let settings = NodeSettings::read_in(&proc, &node_in(&proc)).expect("readable settings");

    // Assert
    assert_eq!(settings.get("http.port"), Some("9300"));
}

#[test]
fn read_in_resolves_a_placeholder_whose_default_is_a_placeholder() {
    // Arrange: stopping at the first `}` read `${A:${B}` as the placeholder and mangled it.
    let proc = scratch_tree("elasticsearch-settings-placeholder-nested", &["600/root"]);
    write(&proc, "600/cmdline", SERVER_ARGV);
    write(&proc, "600/environ", "B=9400\0");
    write(&proc, CONFIG_FILE, "http.port: ${A:${B}}\n");

    // Act
    let settings = NodeSettings::read_in(&proc, &node_in(&proc)).expect("readable settings");

    // Assert
    assert_eq!(settings.get("http.port"), Some("9400"));
}
