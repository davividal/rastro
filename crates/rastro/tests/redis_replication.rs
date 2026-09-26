//! What `INFO replication` says about who a server copies, and who copies it.

use rastro::collectors::redis::InfoReplication;
use rastro_collector::Observation;
use rastro_fingerprint::Volatility;

mod support;

use support::observation::{field, is_null, items_of, text};

const REPLICA: &str = "# Replication\r\n\
role:slave\r\n\
master_host:10.0.0.1\r\n\
master_port:6379\r\n\
master_link_status:up\r\n\
master_last_io_seconds_ago:1\r\n\
slave_repl_offset:5210\r\n\
connected_slaves:0\r\n";

const MASTER: &str = "# Replication\r\n\
role:master\r\n\
connected_slaves:2\r\n\
slave0:ip=10.0.0.3,port=6380,state=online,offset=5210,lag=0\r\n\
slave1:ip=10.0.0.2,port=6379,state=online,offset=5210,lag=1\r\n\
master_repl_offset:5210\r\n";

fn replication(text: &str) -> Observation {
    Observation::from(&InfoReplication::parse(text).expect("a real reply"))
}

#[test]
fn a_replica_names_the_master_it_copies() {
    // Act
    let replication = replication(REPLICA);

    // Assert: a box copying something nobody wrote down is the finding this is here for.
    assert_eq!(text(&field(&replication, "role")), "slave");
    assert_eq!(text(&field(&replication, "master")), "10.0.0.1:6379");
    assert_eq!(text(&field(&replication, "master_link_status")), "up");
}

#[test]
fn a_masters_replicas_are_listed_in_a_fixed_order() {
    // Act
    let replication = replication(MASTER);

    // Assert: sorted, since the numbering is the order they happened to connect in.
    let replicas: Vec<String> = items_of(&field(&replication, "replicas"))
        .iter()
        .map(text)
        .collect();
    assert_eq!(replicas, ["10.0.0.2:6379", "10.0.0.3:6380"]);
    assert!(is_null(&field(&replication, "master")));
}

#[test]
fn what_moves_on_its_own_is_volatile() {
    // Act
    let replica = replication(REPLICA);
    let master = replication(MASTER);

    // Assert: a link drops and comes back, and replicas connect and leave, with nobody
    // touching the configuration of this box.
    assert_eq!(
        field(&replica, "master_link_status").volatility(),
        Volatility::Volatile
    );
    assert_eq!(
        field(&master, "replicas").volatility(),
        Volatility::Volatile
    );
    assert_eq!(field(&master, "role").volatility(), Volatility::Stable);
}

#[test]
fn a_reply_without_a_role_is_refused() {
    // Act
    let result = InfoReplication::parse("# Replication\r\nconnected_slaves:0\r\n");

    // Assert
    assert!(result.is_err(), "{result:?}");
}

#[test]
fn an_ipv6_address_is_bracketed_so_its_port_stays_readable() {
    // Act
    let replication = replication(
        "# Replication\r\nrole:master\r\nslave0:ip=fd00::2,port=6379,state=online,offset=1,lag=0\r\n",
    );

    // Assert
    let replicas: Vec<String> = items_of(&field(&replication, "replicas"))
        .iter()
        .map(text)
        .collect();
    assert_eq!(replicas, ["[fd00::2]:6379"]);
}
