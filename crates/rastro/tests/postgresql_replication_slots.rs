//! Reading a cluster's replication slots, without a cluster to read them from.
//!
//! Only the stable subset is read: a slot's identity and shape, never its LSNs or active
//! flag, which move as it works. A slot appearing is a subscription pointed at this cluster.

use rastro::collectors::postgresql::{PsqlReplicationSlots, ReplicationSlot};

/// The six stable columns the collector's query asks for: a physical slot and a logical one.
const SLOTS: &str = "\
standby_1,,physical,,f,f
sub_slot,pgoutput,logical,orders,f,t
";

fn parsed(csv: &str) -> Vec<ReplicationSlot> {
    PsqlReplicationSlots::parse(csv)
        .expect("this output is well formed")
        .slots()
        .to_vec()
}

fn named<'a>(slots: &'a [ReplicationSlot], name: &str) -> &'a ReplicationSlot {
    slots
        .iter()
        .find(|slot| slot.name == name)
        .expect("the fixture has this slot")
}

#[test]
fn parse_reads_a_physical_slot_with_no_plugin_or_database() {
    // Act
    let slots = parsed(SLOTS);

    // Assert: a physical slot has neither a plugin nor a database, and an empty column is an
    // absent value rather than an empty string.
    let physical = named(&slots, "standby_1");
    assert_eq!(physical.slot_type, "physical");
    assert_eq!(physical.plugin, None);
    assert_eq!(physical.database, None);
}

#[test]
fn parse_reads_a_logical_slot_with_its_plugin_and_database() {
    // Act
    let slots = parsed(SLOTS);

    // Assert
    let logical = named(&slots, "sub_slot");
    assert_eq!(logical.slot_type, "logical");
    assert_eq!(logical.plugin.as_deref(), Some("pgoutput"));
    assert_eq!(logical.database.as_deref(), Some("orders"));
    assert!(logical.two_phase);
}

#[test]
fn parse_reads_a_cluster_with_no_slots_as_empty() {
    // Act & Assert: most clusters hold no slot, so nothing is the common state rather than a
    // failed read.
    assert!(
        PsqlReplicationSlots::parse("")
            .expect("empty is well formed")
            .slots()
            .is_empty()
    );
}

#[test]
fn parse_reads_the_five_column_shape_without_two_phase() {
    // Arrange: PostgreSQL 13 and earlier have no two_phase column, so the read is five
    // columns rather than six.
    let pg13 = "standby_1,,physical,,f\nsub_slot,pgoutput,logical,orders,f\n";

    // Act
    let slots = parsed(pg13);

    // Assert: the slot is read, and two_phase defaults to false where the server cannot
    // report it, rather than the whole facet failing for want of one column.
    assert_eq!(slots.len(), 2);
    assert!(!named(&slots, "sub_slot").two_phase);
}

#[test]
fn new_refuses_one_slot_reported_twice() {
    // Act & Assert: slot_name is unique in the catalog, so a repeat means two reads were
    // spliced.
    let repeated = "s,,physical,,f,f\ns,,physical,,f,f\n";
    assert!(PsqlReplicationSlots::parse(repeated).is_err());
}
