//! Each captured node's settings, read from what the box shows before any request.
//!
//! They decide where a node may be asked and what is sealed, so every node of every cell must
//! read. Whether its listener wants TLS is not among them: the listener says so itself. One test
//! per cell of `docs/elasticsearch-matrix.md`.

use rastro::collectors::elasticsearch::{NodeSettings, ResidentNode};

mod support;

use support::captured_cell::{captured_proc, installs_of};

/// The refusal of each node of a cell whose settings could not be read, `None` for one that read.
fn read(cell: &str) -> Vec<Option<String>> {
    let proc = captured_proc(cell, installs_of(cell));
    ResidentNode::all_in(&proc)
        .iter()
        .map(|node| {
            NodeSettings::read_in(&proc, node)
                .err()
                .map(|unread| unread.reason().to_owned())
        })
        .collect()
}

macro_rules! cell {
    ($name:ident, $cell:literal) => {
        #[test]
        fn $name() {
            // Act
            let read = read($cell);

            // Assert
            assert_eq!(read, vec![None; installs_of($cell).len()]);
        }
    };
}

cell!(settings_of_cell_01_a_7_17_package, "01");
cell!(settings_of_cell_02_an_8_19_package_on_tls, "02");
cell!(settings_of_cell_03_an_8_19_package_with_security_off, "03");
cell!(
    settings_of_cell_04_a_9_5_package_with_es_path_conf_elsewhere,
    "04"
);
cell!(settings_of_cell_05_a_daemonised_9_4_tarball, "05");
cell!(settings_of_cell_06_the_blind_spot, "06");
cell!(settings_of_cell_07_a_daemonised_7_17_tarball, "07");
cell!(settings_of_cell_08_a_9_5_tarball_under_its_own_unit, "08");
cell!(settings_of_cell_09_a_9_5_container, "09");
cell!(settings_of_cell_10_an_8_19_container_with_bind_mounts, "10");
cell!(
    settings_of_cell_11_a_9_4_container_with_a_symlinked_file,
    "11"
);
cell!(settings_of_cell_12_an_8_19_container_on_tls, "12");
cell!(settings_of_cell_13_two_9_5_containers, "13");
cell!(settings_of_cell_14_two_7_17_tarballs, "14");
cell!(settings_of_cell_15_an_8_19_container_with_no_master, "15");
cell!(settings_of_cell_16_a_6_8_container, "16");
cell!(settings_of_cell_17_a_7_10_oss_container, "17");
cell!(settings_of_cell_18_an_8_15_package, "18");
cell!(settings_of_cell_19_a_9_2_container, "19");
cell!(settings_of_cell_21_a_package_and_a_container, "21");
cell!(settings_of_cell_22_two_9_5_clusters, "22");
cell!(settings_of_cell_23_two_7_17_clusters_with_security_on, "23");
cell!(settings_of_cell_24_a_9_5_container, "24");
cell!(
    settings_of_cell_25_a_9_4_package_on_plain_http_with_security_on,
    "25"
);
cell!(settings_of_cell_26_an_8_19_container_auditing, "26");
cell!(settings_of_cell_27_security_off_beside_a_tls_block, "27");
cell!(settings_of_cell_28_a_survivor_of_a_lost_master, "28");
cell!(settings_of_cell_29_a_daemonised_node_on_custom_ports, "29");
cell!(settings_of_cell_30_an_8_19_package_on_mutual_tls, "30");
cell!(settings_of_cell_31_a_package_upgraded_under_its_node, "31");
