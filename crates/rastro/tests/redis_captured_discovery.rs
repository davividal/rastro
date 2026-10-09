//! What each captured server is keyed by, where rastro would connect to it, and from which network
//! namespace, from `/proc` alone.
//!
//! A server in a container listens only in its own network namespace, so its sockets are in that
//! namespace's tables and not the host's, and it is reached from a thread that joins it. One case
//! per cell of `docs/redis-matrix.md`.

use rastro::collectors::redis::discover;

mod support;

use support::captured_redis_cell::captured_box;

/// Each server of the cell as its key, where it is reached, and whether from rastro's own
/// network namespace, in key order.
fn discovered_in(cell: &str) -> Vec<(String, String, bool)> {
    let captured = captured_box(cell);
    let mut discovered: Vec<(String, String, bool)> = discover(&captured.proc)
        .into_iter()
        .map(|server| {
            let reach = match &server.reach {
                Ok(target) => target.to_string(),
                Err(reason) => format!("unreached: {reason}"),
            };
            (server.key, reach, server.namespace.is_ours())
        })
        .collect();
    discovered.sort();
    discovered
}

macro_rules! cell {
    ($name:ident, $cell:literal, [$(($key:literal, $reach:literal, $ours:literal)),* $(,)?]) => {
        #[test]
        fn $name() {
            // Act
            let discovered = discovered_in($cell);

            // Assert
            let expected: Vec<(String, String, bool)> =
                vec![$(($key.to_owned(), $reach.to_owned(), $ours)),*];
            assert_eq!(discovered, expected);
        }
    };
}

cell!(
    cell_01_the_package_on_loopback,
    "01",
    [("6379", "127.0.0.1:6379", true)]
);
cell!(
    cell_02_the_oldest_supported_package,
    "02",
    [("6379", "127.0.0.1:6379", true)]
);
cell!(
    cell_03_listening_everywhere_is_reached_on_loopback,
    "03",
    [("6379", "127.0.0.1:6379", true)]
);
cell!(
    cell_09_a_unix_socket_alone,
    "09",
    [(
        "/run/redis/redis-server.sock",
        "/run/redis/redis-server.sock",
        true
    )]
);
cell!(
    cell_10_the_plain_port_the_title_names_beside_tls,
    "10",
    [("6379", "127.0.0.1:6380", true)]
);
cell!(
    cell_12_the_templates_two_instances,
    "12",
    [
        ("6380", "127.0.0.1:6380", true),
        ("6381", "127.0.0.1:6381", true)
    ]
);
cell!(
    cell_15_a_custom_port,
    "15",
    [("6390", "127.0.0.1:6390", true)]
);
cell!(
    cell_19_a_server_started_by_hand,
    "19",
    [("6379", "127.0.0.1:6379", true)]
);
cell!(cell_21_a_stopped_server_is_not_there, "21", []);
cell!(cell_22_a_sentinel_is_not_a_server, "22", []);
cell!(
    cell_23_a_container_is_reached_inside_its_namespace,
    "23",
    [("6379", "127.0.0.1:6379", false)]
);
cell!(
    cell_24_a_published_container_is_reached_inside_its_namespace_not_through_the_proxy,
    "24",
    [("6379", "127.0.0.1:6379", false)]
);
cell!(
    cell_25_a_container_with_a_mounted_file,
    "25",
    [("6379", "127.0.0.1:6379", false)]
);
cell!(
    cell_26_a_valkey_container,
    "26",
    [("6379", "127.0.0.1:6379", false)]
);
cell!(
    cell_27_valkey_under_its_own_unit,
    "27",
    [("6379", "127.0.0.1:6379", true)]
);
cell!(
    cell_29_valkey_on_a_unix_socket,
    "29",
    [(
        "/run/valkey-29/valkey.sock",
        "/run/valkey-29/valkey.sock",
        true
    )]
);
cell!(
    cell_30_a_valkey_container_with_tls_beside_plain,
    "30",
    [("6379", "127.0.0.1:6380", false)]
);
cell!(
    cell_31_two_servers_on_one_port_in_two_namespaces,
    "31",
    [
        ("0.0.0.0:6379", "127.0.0.1:6379", false),
        ("127.0.0.1:6379", "127.0.0.1:6379", true)
    ]
);
cell!(
    cell_35_redis_5_listening_everywhere,
    "35",
    [("6379", "127.0.0.1:6379", true)]
);
