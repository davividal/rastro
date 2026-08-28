//! Reading `postmaster.pid` into the observed half of a cluster, without a cluster.
//!
//! The layout is fixed by `pidfile.h`: one value per line. The fixtures are shaped like a
//! real running PostgreSQL 17 cluster's file, padding on the status line included, because
//! that padding is exactly the kind of thing a parser gets wrong.

use rastro::collectors::postgresql::{PostmasterPid, PostmasterStatus};

/// A file from a primary that is up and accepting connections. Line 8 is padded to a fixed
/// width by the server, so it carries trailing spaces.
const READY: &str = "\
4242
/var/lib/postgresql/17/main
1700000000
5432
/var/run/postgresql
*
1234567890 42
ready   
";

#[test]
fn parse_reads_the_running_port() {
    // Act
    let postmaster = PostmasterPid::parse(READY).expect("this pid file is well formed");

    // Assert: line 4 is the port the server is actually serving on, which is the reason the
    // file is read at all.
    assert_eq!(postmaster.port, 5432);
}

#[test]
fn parse_reads_the_socket_directory_and_listen_addresses() {
    // Act
    let postmaster = PostmasterPid::parse(READY).expect("well formed");

    // Assert
    assert_eq!(
        postmaster.socket_directory.as_deref(),
        Some("/var/run/postgresql")
    );
    assert_eq!(postmaster.listen_addresses.as_deref(), Some("*"));
}

#[test]
fn parse_reads_a_padded_status_line() {
    // Act
    let postmaster = PostmasterPid::parse(READY).expect("well formed");

    // Assert: the server pads the status to a fixed width, so the trailing spaces have to be
    // trimmed before the word is recognised.
    assert_eq!(postmaster.status, Some(PostmasterStatus::Ready));
}

#[test]
fn parse_reads_a_standby_refusing_connections() {
    // Arrange: a streaming standby with hot_standby = off is up and deliberately refusing
    // connections, which a psql attempt cannot tell from a broken cluster. The pid file can.
    let standby = "4242\n/data\n1700000000\n5432\n/run/postgresql\n*\n0\nstandby \n";

    // Act
    let postmaster = PostmasterPid::parse(standby).expect("well formed");

    // Assert
    assert_eq!(postmaster.status, Some(PostmasterStatus::Standby));
}

#[test]
fn parse_reads_an_empty_socket_directory_as_none() {
    // Arrange: a server listening on TCP only leaves the socket directory line empty.
    let tcp_only = "4242\n/data\n1700000000\n5432\n\n*\n0\nready   \n";

    // Act
    let postmaster = PostmasterPid::parse(tcp_only).expect("well formed");

    // Assert: an empty line is an unset value, not a directory named the empty string.
    assert_eq!(postmaster.socket_directory, None);
}

#[test]
fn parse_refuses_a_file_with_no_port() {
    // Act & Assert: a file too short to carry the port is not a running postmaster's, so it
    // is a failure rather than a cluster observed with no port.
    assert!(PostmasterPid::parse("4242\n/data\n").is_err());
}

#[test]
fn parse_refuses_a_port_that_is_not_a_number() {
    // Act & Assert
    let malformed = "4242\n/data\n1700000000\nnotaport\n/run\n*\n0\nready   \n";
    assert!(PostmasterPid::parse(malformed).is_err());
}
