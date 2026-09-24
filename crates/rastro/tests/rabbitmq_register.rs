//! The box's own register of Erlang nodes, parsed with no epmd to ask.
//!
//! `epmd -names` is the one read in this facet's dispatch that may run before anything is
//! known to be up. It was measured not to start the daemon it fails to reach, while every
//! RabbitMQ CLI tool does start one and leave it behind, which is why the register is asked
//! first and the CLI is never asked speculatively. See `docs/decisions.md`.

use rastro::collectors::rabbitmq::EpmdRegister;

/// What epmd printed on the box the footprint was measured on.
const MEASURED: &str = "epmd: up and running on port 4369 with data:\nname rabbit at port 25672\n";

/// The banner alone, which is epmd running with nothing registered.
const NO_NODES: &str = "epmd: up and running on port 4369 with data:\n";

#[test]
fn parse_reads_a_node_and_its_distribution_port() {
    // Act
    let registered = EpmdRegister::parse(MEASURED).expect("well formed");

    // Assert
    assert_eq!(registered.len(), 1);
    assert_eq!(registered[0].name, "rabbit");
    assert_eq!(registered[0].distribution_port, 25672);
}

#[test]
fn parse_reads_a_running_epmd_with_nothing_registered() {
    // Act & Assert: epmd outlives the node that started it, so an empty register is an
    // ordinary state and not a failed read.
    assert_eq!(EpmdRegister::parse(NO_NODES).expect("well formed").len(), 0);
}

#[test]
fn parse_refuses_output_that_carries_no_banner() {
    // Act & Assert: `epmd: Cannot connect to local epmd` must never read as a register with
    // no nodes. The two are opposite states, and conflating them would report a box with no
    // port mapper as a box with no RabbitMQ.
    assert!(EpmdRegister::parse("epmd: Cannot connect to local epmd\n").is_err());
}

#[test]
fn parse_refuses_a_row_whose_port_is_not_a_port() {
    // Act & Assert
    let malformed = "epmd: up and running on port 4369 with data:\nname rabbit at port fifty\n";
    assert!(EpmdRegister::parse(malformed).is_err());
}

#[test]
fn parse_refuses_a_row_that_names_no_node() {
    // Act & Assert: an empty name addresses nothing, so it is a misread row rather than a
    // node called nothing.
    let malformed = "epmd: up and running on port 4369 with data:\nname  at port 25672\n";
    assert!(EpmdRegister::parse(malformed).is_err());
}

#[test]
fn parse_sorts_the_nodes_it_found() {
    // Arrange: epmd prints in registration order, which is the order the nodes happened to
    // start in and therefore moves between boots of an unchanged box.
    let two = "epmd: up and running on port 4369 with data:\n\
               name rabbit at port 25672\n\
               name aardvark at port 25673\n";

    // Act
    let registered = EpmdRegister::parse(two).expect("well formed");

    // Assert
    let names: Vec<&str> = registered.iter().map(|node| node.name.as_str()).collect();
    assert_eq!(names, ["aardvark", "rabbit"]);
}
