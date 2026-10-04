//! Where each captured node is installed and configured, read from its real `/proc`.
//!
//! One test per cell of `docs/elasticsearch-matrix.md`. The process files
//! are the real nodes', so a launcher, a daemonised start or an argv shape no report described
//! fails here rather than on a box. The rule (`docs/decisions.md`): the server process alone
//! says where the node is, `-Des.path.home` or the directory above `--module-path …/lib`, and
//! `-Des.path.conf`, then `ES_PATH_CONF` in its environment, then `<home>/config`.

use std::path::PathBuf;

use rastro::collectors::elasticsearch::ResidentNode;

mod support;

use support::captured_cell::{captured_nodes, captured_proc, installs_of};

/// Every node of the cell, as `(home, config)`, in the order of the cell's table.
fn paths_read(cell: &str) -> Vec<(Option<PathBuf>, Option<PathBuf>)> {
    let installs = installs_of(cell);
    let proc = captured_proc(cell, installs);
    let mut read: Vec<(Option<PathBuf>, Option<PathBuf>)> = ResidentNode::all_in(&proc)
        .iter()
        .map(|node| {
            (
                node.home().map(PathBuf::from),
                node.config().map(PathBuf::from),
            )
        })
        .collect();
    read.sort();
    read
}

fn paths_set_up(cell: &str) -> Vec<(Option<PathBuf>, Option<PathBuf>)> {
    let installs = installs_of(cell);
    let names: Vec<&str> = installs.iter().map(|install| install.node).collect();
    assert_eq!(
        captured_nodes(cell),
        names,
        "the cell's table names every captured node"
    );

    let mut set_up: Vec<(Option<PathBuf>, Option<PathBuf>)> = installs
        .iter()
        .map(|install| {
            (
                Some(PathBuf::from(install.home)),
                Some(PathBuf::from(install.config)),
            )
        })
        .collect();
    set_up.sort();
    set_up
}

macro_rules! cell {
    ($name:ident, $cell:literal) => {
        #[test]
        fn $name() {
            // Arrange
            let expected = paths_set_up($cell);

            // Act
            let read = paths_read($cell);

            // Assert
            assert_eq!(read, expected);
        }
    };
}

cell!(all_in_reads_cell_01_a_7_17_package_under_systemd, "01");
cell!(all_in_reads_cell_02_an_8_19_package_with_security_on, "02");
cell!(all_in_reads_cell_03_an_8_19_package_on_lvm, "03");
cell!(
    all_in_reads_cell_04_a_9_5_package_with_es_path_conf_elsewhere,
    "04"
);
cell!(all_in_reads_cell_05_a_daemonised_9_4_tarball, "05");
cell!(all_in_reads_cell_06_a_daemonised_8_19_tarball, "06");
cell!(all_in_reads_cell_07_a_daemonised_7_17_tarball, "07");
cell!(all_in_reads_cell_08_a_9_5_tarball_under_its_own_unit, "08");
cell!(all_in_reads_cell_09_a_9_5_container_on_a_volume, "09");
cell!(
    all_in_reads_cell_10_an_8_19_container_with_bind_mounts,
    "10"
);
cell!(
    all_in_reads_cell_11_a_9_4_container_with_a_symlinked_file,
    "11"
);
cell!(
    all_in_reads_cell_12_an_8_19_container_with_security_on,
    "12"
);
cell!(all_in_reads_cell_13_two_9_5_containers_in_one_cluster, "13");
cell!(all_in_reads_cell_14_two_7_17_tarballs_on_the_host, "14");
cell!(all_in_reads_cell_15_an_8_19_container_with_no_master, "15");
cell!(all_in_reads_cell_16_a_6_8_container, "16");
cell!(all_in_reads_cell_17_a_7_10_oss_container, "17");
cell!(all_in_reads_cell_18_an_8_15_package, "18");
cell!(all_in_reads_cell_19_a_9_2_container, "19");
cell!(all_in_reads_cell_20_a_stopped_package_as_no_node, "20");
cell!(all_in_reads_cell_21_a_package_and_a_container, "21");
cell!(all_in_reads_cell_22_two_9_5_clusters_in_containers, "22");
cell!(all_in_reads_cell_23_two_7_17_clusters_on_the_host, "23");
cell!(
    all_in_reads_cell_24_a_9_5_container_with_a_foreign_key,
    "24"
);
cell!(all_in_reads_cell_25_a_9_4_package_with_plain_http, "25");
cell!(all_in_reads_cell_26_an_8_19_container_with_audit_on, "26");
