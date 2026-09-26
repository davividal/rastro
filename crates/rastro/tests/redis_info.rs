//! What a server says about itself in `INFO server`.

use rastro::collectors::redis::{InfoServer, ServerKind};

/// Debian 12's own package, trimmed to the fields around the ones read. Measured.
const DEBIAN_12: &str = "# Server\r\n\
redis_version:7.0.15\r\n\
redis_git_sha1:00000000\r\n\
redis_mode:standalone\r\n\
os:Linux 6.1.0-18-amd64 x86_64\r\n\
process_id:412\r\n\
run_id:3c1f8f2b0f1d4a7e9d6b5c4a3f2e1d0c9b8a7f6e\r\n\
tcp_port:6379\r\n\
uptime_in_seconds:81\r\n\
executable:/usr/bin/redis-server\r\n\
config_file:/etc/redis/redis.conf\r\n";

/// The official valkey image: a `redis_version` frozen at the release valkey forked from, and
/// the real version beside it. Measured.
const VALKEY: &str = "# Server\r\n\
redis_version:7.2.4\r\n\
server_name:valkey\r\n\
valkey_version:9.1.2\r\n\
server_mode:standalone\r\n\
executable:/usr/local/bin/valkey-server\r\n\
config_file:\r\n";

/// The field host's version, whose `INFO` predates both `executable` and `server_name`.
const REDIS_5: &str = "# Server\r\n\
redis_version:5.0.3\r\n\
redis_mode:standalone\r\n\
config_file:/etc/redis/redis.conf\r\n";

#[test]
fn a_redis_server_reports_its_version_mode_and_where_it_came_from() {
    // Act
    let info = InfoServer::parse(DEBIAN_12).expect("a real reply");

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
    let info = InfoServer::parse(VALKEY).expect("a real reply");

    // Assert: `redis_version` would say 7.2.4 about a 9.1.2 server.
    assert_eq!(info.kind, ServerKind::Valkey);
    assert_eq!(info.version, "9.1.2");
    assert_eq!(info.mode.as_deref(), Some("standalone"));
}

#[test]
fn a_server_started_without_a_file_has_no_config_file() {
    // Act & Assert: redis prints the field empty, which is not a path.
    assert_eq!(
        InfoServer::parse(VALKEY).expect("a real reply").config_file,
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
