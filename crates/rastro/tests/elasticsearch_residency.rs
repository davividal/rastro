//! Which Elasticsearch servers are running, read from a `/proc` a test built.
//!
//! This is the gate every request hangs on: rastro sends nothing to a listener whose holder it
//! has not identified as an Elasticsearch server, so the identification has to be right in
//! both directions, across the two argv shapes the supported versions start with.

use std::path::Path;

use rastro::collectors::elasticsearch::ResidentNode;

mod support;

use support::fs_tree::{scratch_tree, write};

/// A 7.17 server, trimmed to the tokens that matter.
///
/// One process, the main class on the classpath. The real argv is forty tokens of JVM tuning,
/// and on the docker image each setting passed as an environment variable follows as `-E`.
const SERVER_7_ARGV: &str = "/usr/share/elasticsearch/jdk/bin/java\0-Xms512m\0\
    -Des.path.home=/usr/share/elasticsearch\0-Des.path.conf=/usr/share/elasticsearch/config\0\
    -Des.distribution.type=docker\0-cp\0/usr/share/elasticsearch/lib/*\0\
    org.elasticsearch.bootstrap.Elasticsearch\0-Ediscovery.type=single-node\0";

/// An 8.x or 9.x server: the same class, started as a module rather than from the classpath.
///
/// **No `-Des.path.*` and no `-E`**, measured on 8.15.3 by the conformance run: the launcher
/// holds them and hands the server its arguments over a pipe, so they are on the parent's argv.
const SERVER_8_ARGV: &str = "/usr/share/elasticsearch/jdk/bin/java\0-Xms512m\0\
    --module-path\0/usr/share/elasticsearch/lib\0\
    -m\0org.elasticsearch.server/org.elasticsearch.bootstrap.Elasticsearch\0";

/// The server's `stat`, naming its parent in the fourth field.
fn stat_with_parent(process_id: &str, parent: &str) -> String {
    format!("{process_id} (java) S {parent} 97 1 0 -1 4194560 0 0 0 0 0 0 0 0 20 0 80 0\n")
}

/// The 8.x launcher that forks the server. It holds no listener and is not a node.
const LAUNCHER_8_ARGV: &str = "/usr/share/elasticsearch/jdk/bin/java\0\
    -Dcli.script=/usr/share/elasticsearch/bin/elasticsearch\0\
    -Des.path.home=/usr/share/elasticsearch\0-Des.path.conf=/etc/elasticsearch\0\
    -cp\0/usr/share/elasticsearch/lib/*:/usr/share/elasticsearch/lib/cli-launcher/*\0\
    org.elasticsearch.launcher.CliToolLauncher\0";

/// OpenSearch, the fork: same shape, its own class and property names, and not what an
/// Elasticsearch request should be sent to.
const OPENSEARCH_ARGV: &str = "/usr/share/opensearch/jdk/bin/java\0\
    -Dopensearch.path.home=/usr/share/opensearch\0-Dopensearch.path.conf=/etc/opensearch\0\
    -cp\0/usr/share/opensearch/lib/*\0org.opensearch.bootstrap.OpenSearch\0";

#[test]
fn all_in_finds_a_7_server_started_from_the_classpath() {
    // Arrange
    let proc = scratch_tree("elasticsearch-residency-7", &["812"]);
    write(&proc, "812/cmdline", SERVER_7_ARGV);

    // Act
    let nodes = ResidentNode::all_in(&proc);

    // Assert
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].process_id(), 812);
    assert_eq!(nodes[0].home(), Some(Path::new("/usr/share/elasticsearch")));
    assert_eq!(
        nodes[0].config(),
        Some(Path::new("/usr/share/elasticsearch/config"))
    );
}

#[test]
fn all_in_finds_an_8_server_started_as_a_module_and_not_its_launcher() {
    // Arrange
    let proc = scratch_tree("elasticsearch-residency-8", &["40", "97"]);
    write(&proc, "40/cmdline", LAUNCHER_8_ARGV);
    write(&proc, "97/cmdline", SERVER_8_ARGV);
    write(&proc, "97/stat", &stat_with_parent("97", "40"));

    // Act
    let nodes = ResidentNode::all_in(&proc);

    // Assert: the paths are the launcher's, since the server's own argv carries none.
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].process_id(), 97);
    assert_eq!(nodes[0].home(), Some(Path::new("/usr/share/elasticsearch")));
    assert_eq!(nodes[0].config(), Some(Path::new("/etc/elasticsearch")));
}

#[test]
fn all_in_takes_no_paths_from_a_parent_that_is_not_the_launcher() {
    // Arrange: a server started by some other JVM that carries a config path of its own, which
    // is not the node's and must not be read as though it were.
    let proc = scratch_tree("elasticsearch-residency-orphan", &["30", "97"]);
    write(
        &proc,
        "30/cmdline",
        "/usr/bin/java\0-Des.path.conf=/opt/other\0-cp\0supervisor.jar\0com.example.Supervisor\0",
    );
    write(&proc, "97/cmdline", SERVER_8_ARGV);
    write(&proc, "97/stat", &stat_with_parent("97", "30"));

    // Act
    let nodes = ResidentNode::all_in(&proc);

    // Assert: a node still, with nothing claimed about where it is configured.
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].config(), None);
}

#[test]
fn all_in_does_not_mistake_opensearch_for_elasticsearch() {
    // Arrange
    let proc = scratch_tree("elasticsearch-residency-opensearch", &["300"]);
    write(&proc, "300/cmdline", OPENSEARCH_ARGV);

    // Act
    let nodes = ResidentNode::all_in(&proc);

    // Assert
    assert!(nodes.is_empty());
}

#[test]
fn all_in_does_not_match_the_class_name_inside_another_argument() {
    // Arrange: a script that merely mentions the class, as a grep or a wrapper's log line would.
    let proc = scratch_tree("elasticsearch-residency-mention", &["55"]);
    write(
        &proc,
        "55/cmdline",
        "/usr/bin/grep\0-r\0org.elasticsearch.bootstrap.Elasticsearch-notes\0/var/log\0",
    );

    // Act
    let nodes = ResidentNode::all_in(&proc);

    // Assert
    assert!(nodes.is_empty());
}

#[test]
fn all_in_does_not_mistake_a_process_naming_the_class_as_an_argument_for_a_server() {
    // Arrange: found by the conformance run, whose own `pgrep -f` was read as a third node. The
    // class is a whole argument here, so matching whole arguments is not enough: it has to be
    // the main class a JVM was started with.
    let proc = scratch_tree("elasticsearch-residency-pgrep", &["70", "71"]);
    write(
        &proc,
        "70/cmdline",
        "pgrep\0-f\0org.elasticsearch.bootstrap.Elasticsearch\0",
    );
    write(
        &proc,
        "71/cmdline",
        "/usr/bin/java\0-jar\0tool.jar\0org.elasticsearch.bootstrap.Elasticsearch\0",
    );

    // Act
    let nodes = ResidentNode::all_in(&proc);

    // Assert
    assert!(nodes.is_empty(), "{nodes:?}");
}

#[test]
fn all_in_lists_several_servers_in_process_id_order() {
    // Arrange: two nodes on one box, created so directory order is not the answer.
    let proc = scratch_tree("elasticsearch-residency-two", &["9000", "120"]);
    write(&proc, "9000/cmdline", SERVER_8_ARGV);
    write(&proc, "120/cmdline", SERVER_7_ARGV);

    // Act
    let nodes = ResidentNode::all_in(&proc);

    // Assert
    let process_ids: Vec<u32> = nodes.iter().map(ResidentNode::process_id).collect();
    assert_eq!(process_ids, [120, 9000]);
}

#[test]
fn all_in_keeps_a_server_whose_paths_are_not_in_its_argv() {
    // Arrange: a server started by hand without the launcher's `-D` properties.
    let proc = scratch_tree("elasticsearch-residency-bare", &["7"]);
    write(
        &proc,
        "7/cmdline",
        "java\0-cp\0lib/*\0org.elasticsearch.bootstrap.Elasticsearch\0",
    );

    // Act
    let nodes = ResidentNode::all_in(&proc);

    // Assert: a node still, with nothing claimed about where it lives.
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].home(), None);
    assert_eq!(nodes[0].config(), None);
}

#[test]
fn all_in_finds_nothing_in_an_unreadable_process_table() {
    // Act
    let nodes = ResidentNode::all_in(Path::new("/nonexistent/proc"));

    // Assert
    assert!(nodes.is_empty());
}

#[test]
fn all_in_finds_a_server_with_an_argument_that_is_not_utf_8() {
    // Arrange: a Latin-1 install path. Read as text the whole argv failed, and the server
    // silently stopped being a node, which could leave the facet `absent` on a box running one.
    let proc = scratch_tree("elasticsearch-residency-latin1", &["812"]);
    let mut argv =
        b"/usr/share/elasticsearch/jdk/bin/java\0-Des.path.home=/opt/\xe9lastic\0".to_vec();
    argv.extend_from_slice(b"-cp\0/opt/lib/*\0org.elasticsearch.bootstrap.Elasticsearch\0");
    std::fs::write(proc.join("812/cmdline"), argv).expect("a writable fixture");

    // Act
    let nodes = ResidentNode::all_in(&proc);

    // Assert
    assert_eq!(nodes.len(), 1);
    assert!(!nodes[0].launch_arguments_are_exact());
}

#[test]
fn all_in_reads_no_further_than_the_main_class_a_jvm_was_started_with() {
    // Arrange: found by review. Everything after a JVM's main class is that application's own
    // argument, so a `-cp` and the server's class appearing there are not the JVM's.
    let proc = scratch_tree("elasticsearch-residency-after-main", &["90"]);
    write(
        &proc,
        "90/cmdline",
        "/usr/bin/java\0-cp\0app.jar\0com.example.Main\0-cp\0ignored\0\
         org.elasticsearch.bootstrap.Elasticsearch\0",
    );

    // Act
    let nodes = ResidentNode::all_in(&proc);

    // Assert
    assert!(nodes.is_empty(), "{nodes:?}");
}

#[test]
fn all_in_finds_a_server_whose_classpath_is_not_the_last_option() {
    // Arrange: the main class is the first argument that is not an option or an option's value,
    // wherever the classpath came in the options before it.
    let proc = scratch_tree("elasticsearch-residency-options-after-cp", &["91"]);
    write(
        &proc,
        "91/cmdline",
        "/usr/share/elasticsearch/jdk/bin/java\0-cp\0/usr/share/elasticsearch/lib/*\0\
         -Des.path.home=/usr/share/elasticsearch\0-Xms1g\0\
         org.elasticsearch.bootstrap.Elasticsearch\0",
    );

    // Act
    let nodes = ResidentNode::all_in(&proc);

    // Assert
    assert_eq!(nodes.len(), 1);
}

#[test]
fn all_in_takes_the_last_of_a_repeated_path_property_as_the_jvm_does() {
    // Arrange: found by review, and confirmed there on OpenJDK 11 to 25: with `-Done=first
    // -Done=second` the JVM's property is `second`. Taking the first would read another file
    // than the node's, the one the TLS gate decides from.
    let proc = scratch_tree("elasticsearch-residency-repeated-property", &["812"]);
    write(
        &proc,
        "812/cmdline",
        "/usr/share/elasticsearch/jdk/bin/java\0-Des.path.conf=/etc/first\0\
         -Des.path.home=/opt/first\0-Des.path.conf=/etc/second\0-Des.path.home=/opt/second\0\
         -cp\0/usr/share/elasticsearch/lib/*\0org.elasticsearch.bootstrap.Elasticsearch\0",
    );

    // Act
    let nodes = ResidentNode::all_in(&proc);

    // Assert
    assert_eq!(nodes[0].config(), Some(Path::new("/etc/second")));
    assert_eq!(nodes[0].home(), Some(Path::new("/opt/second")));
}
