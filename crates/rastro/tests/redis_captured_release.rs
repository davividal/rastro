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
cell!(
    cell_12_two_instances_of_the_template,
    "12",
    [(Redis, "8.10.2", true), (Redis, "8.10.2", true)]
);
cell!(
    cell_24_redis_8_10_in_a_published_container,
    "24",
    [(Redis, "8.10.2", true)]
);
cell!(cell_26_valkey_9_1, "26", [(Valkey, "9.1.2", true)]);
cell!(cell_33_redis_7_4, "33", [(Redis, "7.4.11", false)]);
