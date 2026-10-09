//! Which family and release each captured server said it is, and whether rastro supports it.
//!
//! Supported: redis 8.0 to 8.10, by the maintainer's choice, and valkey 7.2, 8.0, 8.1, 9.0 and
//! 9.1, every line endoflife.date lists as maintained. Anything else is read on a best-effort
//! basis and marked `unsupported`. One case per cell of `docs/redis-matrix.md` whose server
//! answered `INFO server`.

use std::fs;
use std::path::Path;

use rastro::collectors::redis::{InfoServer, ServerKind};

mod support;

use support::captured_redis_cell::servers_of;

/// The text of a captured bulk-string reply, as the server sent it.
fn bulk_text(reply: &Path) -> String {
    let bytes = fs::read(reply).expect("a captured reply");
    let text = String::from_utf8(bytes).expect("a UTF-8 reply");
    let (header, body) = text.split_once("\r\n").expect("a RESP header");
    assert!(header.starts_with('$'), "a bulk string: {header}");

    body.strip_suffix("\r\n")
        .expect("a RESP terminator")
        .to_owned()
}

/// Each server of the cell as family, release and whether it is supported, in capture order.
fn releases_of(cell: &str) -> Vec<(ServerKind, String, bool)> {
    servers_of(cell)
        .iter()
        .map(|server| {
            let reply = fs::read_dir(server.join("replies/authenticated"))
                .expect("captured replies")
                .flatten()
                .map(|entry| entry.path())
                .find(|path| path.to_string_lossy().ends_with("INFO-server.resp"))
                .expect("a captured INFO server");
            let identity = InfoServer::parse(&bulk_text(&reply)).expect("a server's INFO");
            let supported = identity.unsupported().is_none();

            (identity.kind, identity.version, supported)
        })
        .collect()
}

macro_rules! cell {
    ($name:ident, $cell:literal, [$(($kind:ident, $version:literal, $supported:literal)),+ $(,)?]) => {
        #[test]
        fn $name() {
            // Act
            let releases = releases_of($cell);

            // Assert
            assert_eq!(
                releases,
                [$((ServerKind::$kind, $version.to_owned(), $supported)),+]
            );
        }
    };
}

cell!(cell_01_redis_8_10, "01", [(Redis, "8.10.2", true)]);
cell!(cell_02_redis_8_0, "02", [(Redis, "8.0.6", true)]);
cell!(cell_03_redis_8_2, "03", [(Redis, "8.2.10", true)]);
cell!(cell_04_redis_8_4, "04", [(Redis, "8.4.7", true)]);
cell!(cell_05_redis_8_6, "05", [(Redis, "8.6.7", true)]);
cell!(cell_06_redis_8_6, "06", [(Redis, "8.6.7", true)]);
cell!(cell_07_redis_8_8, "07", [(Redis, "8.8.3", true)]);
cell!(cell_08_redis_8_8, "08", [(Redis, "8.8.3", true)]);
cell!(
    cell_09_redis_8_10_on_a_unix_socket,
    "09",
    [(Redis, "8.10.2", true)]
);
cell!(cell_10_redis_8_10_with_tls, "10", [(Redis, "8.10.2", true)]);
cell!(
    cell_12_two_instances_of_the_template,
    "12",
    [(Redis, "8.10.2", true), (Redis, "8.10.2", true)]
);
cell!(
    cell_13_redis_8_10_in_cluster_mode,
    "13",
    [(Redis, "8.10.2", true)]
);
cell!(
    cell_14_redis_8_10_without_config,
    "14",
    [(Redis, "8.10.2", true)]
);
cell!(
    cell_15_redis_8_10_with_drop_ins,
    "15",
    [(Redis, "8.10.2", true)]
);
cell!(cell_16_redis_8_10_on_lvm, "16", [(Redis, "8.10.2", true)]);
cell!(
    cell_17_redis_8_10_with_modules,
    "17",
    [(Redis, "8.10.2", true)]
);
cell!(
    cell_18_redis_8_10_default_off,
    "18",
    [(Redis, "8.10.2", true)]
);
cell!(cell_19_redis_8_10_by_hand, "19", [(Redis, "8.10.2", true)]);
cell!(
    cell_20_redis_8_10_by_hand_with_a_password,
    "20",
    [(Redis, "8.10.2", true)]
);
cell!(cell_22_redis_8_10_sentinel, "22", [(Redis, "8.10.2", true)]);
cell!(
    cell_23_redis_8_10_in_a_container,
    "23",
    [(Redis, "8.10.2", true)]
);
cell!(
    cell_24_redis_8_10_in_a_published_container,
    "24",
    [(Redis, "8.10.2", true)]
);
cell!(
    cell_25_redis_8_8_in_a_container,
    "25",
    [(Redis, "8.8.3", true)]
);
cell!(cell_26_valkey_9_1, "26", [(Valkey, "9.1.2", true)]);
cell!(cell_27_valkey_9_0, "27", [(Valkey, "9.0.6", true)]);
cell!(
    cell_28_valkey_8_1_through_a_redis_link,
    "28",
    [(Valkey, "8.1.10", true)]
);
cell!(cell_29_valkey_8_0, "29", [(Valkey, "8.0.11", true)]);
cell!(cell_30_valkey_7_2, "30", [(Valkey, "7.2.14", true)]);
cell!(
    cell_31_one_of_each_family,
    "31",
    [(Redis, "8.10.2", true), (Valkey, "9.1.2", true)]
);
cell!(
    cell_32_debians_own_redis_7_0,
    "32",
    [(Redis, "7.0.15", false)]
);
cell!(cell_33_redis_7_4, "33", [(Redis, "7.4.11", false)]);
cell!(cell_34_redis_6_2, "34", [(Redis, "6.2.24", false)]);
cell!(cell_35_redis_5_0, "35", [(Redis, "5.0.14", false)]);
