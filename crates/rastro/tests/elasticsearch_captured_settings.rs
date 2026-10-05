//! How each captured node may be asked, decided from what the box shows before any request.
//!
//! Only TLS on HTTP is decided before asking, since the wrong protocol is a request the node
//! logs. One test per cell of `docs/elasticsearch-matrix.md`; cell 06 is the blind spot
//! `docs/decisions.md` accepts, TLS switched on by an `-E` its exited launcher took with it.

use rastro::collectors::elasticsearch::{NodeSettings, ResidentNode, Transport};

mod support;

use support::captured_cell::{captured_proc, installs_of};

/// The transport each node of a cell serves, as set up; cell 06 as its file says.
fn set_up(cell: &str) -> Vec<Transport> {
    let tls = match cell {
        "02" | "12" | "21" | "22" | "24" | "26" | "30" => Transport::Tls,
        _ => Transport::Plain,
    };
    let mut transports: Vec<Transport> = installs_of(cell).iter().map(|_| tls).collect();
    // Cell 21 is a package on the host and a 9.5 container, both on TLS.
    transports.sort_by_key(|transport| *transport == Transport::Plain);
    transports
}

fn read(cell: &str) -> Vec<Result<Transport, String>> {
    let proc = captured_proc(cell, installs_of(cell));
    let mut read: Vec<Result<Transport, String>> = ResidentNode::all_in(&proc)
        .iter()
        .map(|node| {
            NodeSettings::read_in(&proc, node)
                .map(|settings| settings.transport())
                .map_err(|unread| unread.reason().to_owned())
        })
        .collect();
    read.sort_by_key(|transport| transport.as_ref().ok() == Some(&Transport::Plain));
    read
}

macro_rules! cell {
    ($name:ident, $cell:literal) => {
        #[test]
        fn $name() {
            // Arrange
            let expected: Vec<Result<Transport, String>> =
                set_up($cell).into_iter().map(Ok).collect();

            // Act
            let read = read($cell);

            // Assert
            assert_eq!(read, expected);
        }
    };
}

cell!(transport_of_cell_01_a_7_17_package, "01");
cell!(transport_of_cell_02_an_8_19_package_on_tls, "02");
cell!(transport_of_cell_03_an_8_19_package_with_security_off, "03");
cell!(
    transport_of_cell_04_a_9_5_package_with_es_path_conf_elsewhere,
    "04"
);
cell!(transport_of_cell_05_a_daemonised_9_4_tarball, "05");
cell!(transport_of_cell_06_the_blind_spot, "06");
cell!(transport_of_cell_07_a_daemonised_7_17_tarball, "07");
cell!(transport_of_cell_08_a_9_5_tarball_under_its_own_unit, "08");
cell!(transport_of_cell_09_a_9_5_container, "09");
cell!(
    transport_of_cell_10_an_8_19_container_with_bind_mounts,
    "10"
);
cell!(
    transport_of_cell_11_a_9_4_container_with_a_symlinked_file,
    "11"
);
cell!(transport_of_cell_12_an_8_19_container_on_tls, "12");
cell!(transport_of_cell_13_two_9_5_containers, "13");
cell!(transport_of_cell_14_two_7_17_tarballs, "14");
cell!(transport_of_cell_15_an_8_19_container_with_no_master, "15");
cell!(transport_of_cell_16_a_6_8_container, "16");
cell!(transport_of_cell_17_a_7_10_oss_container, "17");
cell!(transport_of_cell_18_an_8_15_package, "18");
cell!(transport_of_cell_19_a_9_2_container, "19");
cell!(transport_of_cell_21_a_package_and_a_container, "21");
cell!(transport_of_cell_22_two_9_5_clusters, "22");
cell!(
    transport_of_cell_23_two_7_17_clusters_with_security_on,
    "23"
);
cell!(transport_of_cell_24_a_9_5_container, "24");
cell!(
    transport_of_cell_25_a_9_4_package_on_plain_http_with_security_on,
    "25"
);
cell!(transport_of_cell_26_an_8_19_container_auditing, "26");
cell!(transport_of_cell_28_a_survivor_of_a_lost_master, "28");
cell!(transport_of_cell_29_a_daemonised_node_on_custom_ports, "29");
cell!(transport_of_cell_30_an_8_19_package_on_mutual_tls, "30");
cell!(transport_of_cell_31_a_package_upgraded_under_its_node, "31");
