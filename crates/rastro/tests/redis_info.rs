//! What a server says about itself in `INFO server`.

use rastro::collectors::redis::{InfoServer, ServerKind};

mod support;

use support::captured_redis_cell::captured_reply;

/// The text of a cell's `INFO server` reply, as the captured server sent it.
fn info_of(cell: &str) -> String {
    let reply = captured_reply(cell, "INFO server");
    let (_, body) = reply.split_once("\r\n").expect("a RESP header");

    body.strip_suffix("\r\n")
        .expect("a RESP terminator")
        .to_owned()
}

/// Debian 12's own package, captured as cell 32 of the matrix.
const DEBIAN_12: &str = "32";

/// The official valkey image, captured as cell 26: a `redis_version` frozen at the release valkey
/// forked from, and the real version beside it.
const VALKEY: &str = "26";

/// The field host's version, whose `INFO` predates both `executable` and `server_name`.
const REDIS_5: &str = "# Server\r\n\
redis_version:5.0.3\r\n\
redis_mode:standalone\r\n\
config_file:/etc/redis/redis.conf\r\n";

#[test]
fn a_redis_server_reports_its_version_mode_and_where_it_came_from() {
    // Act
    let info = InfoServer::parse(&info_of(DEBIAN_12)).expect("a real reply");

    // Assert
    assert_eq!(info.kind, ServerKind::Redis);
    assert_eq!(info.version, "7.0.15");
    assert_eq!(info.mode.as_deref(), Some("standalone"));
    assert_eq!(info.executable.as_deref(), Some("/usr/bin/redis-server"));
    assert_eq!(info.config_file.as_deref(), Some("/etc/redis/redis.conf"));
}

#[test]
fn a_valkey_server_is_known_by_its_own_name_and_version() {
    // Act
    let info = InfoServer::parse(&info_of(VALKEY)).expect("a real reply");

    // Assert: `redis_version` would say 7.2.4 about a 9.1.2 server.
    assert_eq!(info.kind, ServerKind::Valkey);
    assert_eq!(info.version, "9.1.2");
    assert_eq!(info.mode.as_deref(), Some("standalone"));
}

#[test]
fn a_server_started_without_a_file_has_no_config_file() {
    // Act & Assert: redis prints the field empty, which is not a path.
    assert_eq!(
        InfoServer::parse(&info_of(VALKEY))
            .expect("a real reply")
            .config_file,
        None
    );
}

#[test]
fn an_old_server_reports_what_it_has() {
    // Act
    let info = InfoServer::parse(REDIS_5).expect("a real reply");

    // Assert
    assert_eq!(info.kind, ServerKind::Redis);
    assert_eq!(info.version, "5.0.3");
    assert_eq!(info.executable, None);
}

#[test]
fn a_reply_without_a_version_is_not_a_server_rastro_can_describe() {
    // Act
    let result = InfoServer::parse("# Server\r\nredis_mode:standalone\r\n");

    // Assert
    assert!(result.is_err(), "{result:?}");
}
