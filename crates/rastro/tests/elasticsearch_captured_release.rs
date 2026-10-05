//! Which release each captured node runs, read before any request is sent.
//!
//! The version decides which release's rules a node is read with and whether it is asked at
//! all, so it cannot come from `GET /`. It comes from the server jar in the node's own
//! filesystem, `<home>/lib/elasticsearch-<version>.jar`, read inside its root: a container's
//! install is not the host's. One test per cell of `docs/elasticsearch-matrix.md`.

use rastro::collectors::elasticsearch::{Release, ResidentNode};

mod support;

use support::captured_cell::{captured_nodes, captured_proc, installs_of};

fn versions_read(cell: &str) -> Vec<Option<String>> {
    let proc = captured_proc(cell, installs_of(cell));
    let mut read: Vec<Option<String>> = ResidentNode::all_in(&proc)
        .iter()
        .map(|node| node.release().map(|release| release.to_string()))
        .collect();
    read.sort();
    read
}

fn versions_set_up(cell: &str) -> Vec<Option<String>> {
    let installs = installs_of(cell);
    let names: Vec<&str> = installs.iter().map(|install| install.node).collect();
    assert_eq!(
        captured_nodes(cell),
        names,
        "the cell's table names every captured node"
    );

    let mut set_up: Vec<Option<String>> = installs
        .iter()
        .map(|install| Some(install.version.to_owned()))
        .collect();
    set_up.sort();
    set_up
}

macro_rules! cell {
    ($name:ident, $cell:literal) => {
        #[test]
        fn $name() {
            // Arrange
            let expected = versions_set_up($cell);

            // Act
            let read = versions_read($cell);

            // Assert
            assert_eq!(read, expected);
        }
    };
}

cell!(release_of_cell_01_a_7_17_package, "01");
cell!(release_of_cell_02_an_8_19_package, "02");
cell!(release_of_cell_03_an_8_19_package_on_lvm, "03");
cell!(release_of_cell_04_a_9_5_package, "04");
cell!(release_of_cell_05_a_daemonised_9_4_tarball, "05");
cell!(release_of_cell_06_a_daemonised_8_19_tarball, "06");
cell!(release_of_cell_07_a_daemonised_7_17_tarball, "07");
cell!(release_of_cell_08_a_9_5_tarball_under_its_own_unit, "08");
cell!(release_of_cell_09_a_9_5_container, "09");
cell!(release_of_cell_10_an_8_19_container, "10");
cell!(release_of_cell_11_a_9_4_container, "11");
cell!(release_of_cell_12_an_8_19_container_with_security_on, "12");
cell!(release_of_cell_13_two_9_5_containers, "13");
cell!(release_of_cell_14_two_7_17_tarballs, "14");
cell!(release_of_cell_15_an_8_19_container_with_no_master, "15");
cell!(release_of_cell_16_a_6_8_container, "16");
cell!(release_of_cell_17_a_7_10_oss_container, "17");
cell!(
    release_of_cell_18_an_8_15_package_without_es_path_home,
    "18"
);
cell!(release_of_cell_19_a_9_2_container, "19");
cell!(release_of_cell_20_no_node, "20");
cell!(release_of_cell_21_a_package_and_a_container, "21");
cell!(release_of_cell_22_two_9_5_clusters, "22");
cell!(release_of_cell_23_two_7_17_clusters, "23");
cell!(release_of_cell_24_a_9_5_container, "24");
cell!(release_of_cell_25_a_9_4_package, "25");
cell!(release_of_cell_26_an_8_19_container_with_audit_on, "26");
cell!(
    release_of_cell_27_an_8_19_package_with_security_off_alone,
    "27"
);
cell!(release_of_cell_28_a_survivor_of_a_lost_master, "28");
cell!(release_of_cell_29_a_daemonised_node_on_custom_ports, "29");
cell!(release_of_cell_30_an_8_19_package_on_mutual_tls, "30");
cell!(release_of_cell_32_nothing_installed, "32");

#[test]
fn release_of_cell_31_is_what_the_node_runs_and_not_what_was_installed_over_it() {
    // Arrange: measured on cell 31, `dpkg -i` of 8.19.22 over a running 8.15.3 node leaves it
    // running, its `GET /` saying 8.15.3, `lib/` holding 8.19.22, and its open server jar the old
    // one, marked ` (deleted)`.
    let proc = captured_proc("31", installs_of("31"));

    // Act
    let nodes = ResidentNode::all_in(&proc);

    // Assert
    assert_eq!(nodes[0].release(), Release::parse("8.15.3"));
    assert_eq!(nodes[0].installed_release(), Release::parse("8.19.22"));
}
