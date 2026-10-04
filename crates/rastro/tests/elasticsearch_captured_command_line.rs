//! Which `-E` settings each captured node was started with, where `/proc` still shows them.
//!
//! On 7.x they are on the server's own argv. On 8.x and 9.x only the launcher holds them, a Java
//! `CliToolLauncher` up to 9.2 and a native `server-launcher` from 9.4, and a node started with
//! `-d` outlives its launcher, so its settings are on no argv at all (cells 05 and 06). One test
//! per cell of `docs/elasticsearch-matrix.md`.

use rastro::collectors::elasticsearch::{Release, ResidentNode};

mod support;

use support::captured_cell::{captured_proc, installs_of};

/// The `-E` settings set up in each node of a cell, in the order of its table; 7.x images turn
/// their environment into `-E` flags, which is why cells 16 and 17 carry one.
fn set_up(cell: &str) -> Vec<Vec<&'static str>> {
    let none: Vec<Vec<&str>> = installs_of(cell).iter().map(|_| Vec::new()).collect();
    match cell {
        "07" => vec![vec!["node.name=cell07", "discovery.type=single-node"]],
        "08" => vec![vec!["node.name=cell08", "http.port=9208"]],
        "11" => vec![vec!["node.name=cell11", "http.port=9351"]],
        "14" => ["a", "b"]
            .iter()
            .map(|node| match *node {
                "a" => vec![
                    "node.name=n14a",
                    "cluster.name=cell14",
                    "http.port=9200",
                    "transport.port=9300",
                    "discovery.seed_hosts=127.0.0.1:9300,127.0.0.1:9301",
                    "cluster.initial_master_nodes=n14a,n14b",
                ],
                _ => vec![
                    "node.name=n14b",
                    "cluster.name=cell14",
                    "http.port=9201",
                    "transport.port=9301",
                    "discovery.seed_hosts=127.0.0.1:9300,127.0.0.1:9301",
                    "cluster.initial_master_nodes=n14a,n14b",
                ],
            })
            .collect(),
        "16" | "17" => vec![vec!["discovery.type=single-node"]],
        "23" => vec![
            vec![
                "cluster.name=cell23a",
                "discovery.type=single-node",
                "xpack.security.enabled=true",
                "http.port=9200",
                "transport.port=9300",
            ],
            vec![
                "cluster.name=cell23b",
                "discovery.type=single-node",
                "xpack.security.enabled=true",
                "http.port=9201",
                "transport.port=9301",
            ],
        ],
        _ => none,
    }
}

/// The `-E` settings among a node's application arguments, in both spellings the captures hold.
fn settings_in(arguments: &[String]) -> Vec<String> {
    let mut settings = Vec::new();
    let mut rest = arguments.iter();
    while let Some(argument) = rest.next() {
        match argument.strip_prefix("-E") {
            Some("") => settings.extend(rest.next().cloned()),
            Some(joined) => settings.push(joined.to_owned()),
            None => {}
        }
    }
    settings
}

fn read(cell: &str) -> Vec<Vec<String>> {
    let proc = captured_proc(cell, installs_of(cell));
    let mut nodes = ResidentNode::all_in(&proc);
    // The table's order: by install, the one thing that tells a cell's nodes apart here.
    nodes.sort_by_key(|node| node.home().map(std::path::Path::to_path_buf));
    nodes
        .iter()
        .map(|node| settings_in(node.application_arguments()))
        .collect()
}

macro_rules! cell {
    ($name:ident, $cell:literal) => {
        #[test]
        fn $name() {
            // Arrange
            let expected: Vec<Vec<String>> = set_up($cell)
                .into_iter()
                .map(|settings| settings.into_iter().map(str::to_owned).collect())
                .collect();

            // Act
            let read = read($cell);

            // Assert
            assert_eq!(read, expected);
        }
    };
}

cell!(command_line_of_cell_01_a_7_17_package, "01");
cell!(command_line_of_cell_02_an_8_19_package, "02");
cell!(command_line_of_cell_04_a_9_5_package, "04");
cell!(command_line_of_cell_05_a_daemonised_9_4_tarball, "05");
cell!(command_line_of_cell_06_a_daemonised_8_19_tarball, "06");
cell!(command_line_of_cell_07_a_daemonised_7_17_tarball, "07");
cell!(
    command_line_of_cell_08_a_9_5_tarball_under_its_own_unit,
    "08"
);
cell!(command_line_of_cell_11_a_9_4_container_started_with_e, "11");
cell!(command_line_of_cell_14_two_7_17_tarballs, "14");
cell!(command_line_of_cell_16_a_6_8_container, "16");
cell!(command_line_of_cell_17_a_7_10_oss_container, "17");
cell!(command_line_of_cell_18_an_8_15_package, "18");
cell!(command_line_of_cell_19_a_9_2_container, "19");
cell!(command_line_of_cell_23_two_7_17_clusters, "23");

#[test]
fn distribution_of_a_9_4_node_is_its_servers_since_the_native_launcher_names_none() {
    // Arrange: cell 11's launcher is the native binary, whose argv carries no `-D` property.
    let proc = captured_proc("11", installs_of("11"));

    // Act
    let nodes = ResidentNode::all_in(&proc);

    // Assert
    assert_eq!(nodes[0].distribution(), Some("docker"));
    assert_eq!(nodes[0].release(), Release::parse("9.4.7"));
}
