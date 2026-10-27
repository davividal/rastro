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
    let mut discovered: Vec<(String, String, bool)> = discover(&captured)
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
    cell_12_the_templates_two_instances,
    "12",
    [
        ("6380", "127.0.0.1:6380", true),
        ("6381", "127.0.0.1:6381", true)
    ]
);
cell!(
    cell_24_a_published_container_is_reached_inside_its_namespace_not_through_the_proxy,
    "24",
    [("6379", "127.0.0.1:6379", false)]
);
cell!(
    cell_26_a_valkey_container,
    "26",
    [("6379", "127.0.0.1:6379", false)]
);
