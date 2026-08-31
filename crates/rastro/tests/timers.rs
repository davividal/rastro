//! Reading systemd's timers, without needing a systemd to read them from.
//!
//! The fixture rows are real rows from `systemctl list-timers --all --output=json` on the
//! development box.

mod support;

use rastro::collectors::timers::{SystemctlTimers, TimerTable, TimersCollector, UnitName};
use rastro_collector::{Collector, Presence};
use rastro_fingerprint::{Content, Observation, Scalar, View};
use support::observation::{field, keys_of};

/// Real rows. Note that `left` repeats `next` and `passed` repeats `last` exactly, which
/// is what this output does and the reason two of the four fields are dropped.
const TIMERS: &str = r#"[
  {"next":1787236810846290,"left":1787236810846290,"last":1787041069750721,"passed":1787041069750721,"unit":"systemd-tmpfiles-clean.timer","activates":"systemd-tmpfiles-clean.service"},
  {"next":1787250207868898,"left":1787250207868898,"last":1787208127657855,"passed":1787208127657855,"unit":"apt-daily.timer","activates":"apt-daily.service"},
  {"next":null,"left":null,"last":null,"passed":null,"unit":"never-ran.timer","activates":"never-ran.service"}
]"#;

fn table() -> TimerTable {
    SystemctlTimers::parse(TIMERS).expect("these fixtures are well formed")
}

fn timer(table: &TimerTable, name: &str) -> rastro::collectors::timers::Timer {
    table
        .timers()
        .get(&UnitName::new(name).expect("a legal unit name"))
        .unwrap_or_else(|| panic!("expected a {name:?} timer"))
        .clone()
}

#[test]
fn parse_reads_the_unit_a_timer_starts() {
    // Act: the one field that survives the diffable view, and the one a diff needs.
    let apt = timer(&table(), "apt-daily.timer");

    // Assert
    assert_eq!(
        apt.activates.map(|unit| unit.as_str().to_owned()),
        Some("apt-daily.service".to_owned())
    );
}

#[test]
fn parse_reads_the_clocks_as_systemd_counts_them() {
    // Act
    let apt = timer(&table(), "apt-daily.timer");

    // Assert: microseconds since the epoch, systemd's own unit, unconverted.
    assert_eq!(
        apt.next_elapse.map(|moment| moment.as_i64()),
        Some(1787250207868898)
    );
    assert_eq!(
        apt.last_trigger.map(|moment| moment.as_i64()),
        Some(1787208127657855)
    );
}

#[test]
fn parse_records_a_timer_that_has_never_fired_as_having_no_last_trigger() {
    // Act
    let never = timer(&table(), "never-ran.timer");

    // Assert
    assert_eq!(never.last_trigger, None);
    assert_eq!(never.next_elapse, None);
}

#[test]
fn parse_keys_timers_by_name_rather_than_keeping_the_order_systemd_used() {
    // Act
    let observation = Observation::from(&table());

    // Assert: `list-timers` orders by next elapse, so its order reshuffles every time a
    // timer fires.
    assert_eq!(
        keys_of(&observation),
        [
            "apt-daily.timer",
            "never-ran.timer",
            "systemd-tmpfiles-clean.timer"
        ]
    );
}

#[test]
fn parse_refuses_a_timer_reported_twice() {
    // Arrange
    let repeated = r#"[
      {"unit":"a.timer","activates":"a.service","next":1,"last":1,"left":1,"passed":1},
      {"unit":"a.timer","activates":"b.service","next":1,"last":1,"left":1,"passed":1}
    ]"#;

    // Act & Assert
    assert!(SystemctlTimers::parse(repeated).is_err());
}

#[test]
fn parse_refuses_output_that_is_not_json() {
    // Act
    let result = SystemctlTimers::parse("NEXT LEFT LAST PASSED UNIT ACTIVATES\n");

    // Assert
    let failure = result.expect_err("a table is not JSON");
    assert!(
        failure.to_string().contains("list-timers"),
        "the message must name the subcommand, got: {failure}"
    );
}

#[test]
fn parse_accepts_a_timer_that_starts_nothing_systemd_can_name() {
    // Arrange: systemd leaves the column empty rather than failing.
    let orphan = r#"[{"unit":"orphan.timer","activates":"","next":null,"last":null,"left":null,"passed":null}]"#;

    // Act
    let table = SystemctlTimers::parse(orphan).expect("an empty column is not a failure");

    // Assert
    assert_eq!(timer(&table, "orphan.timer").activates, None);
}

#[test]
fn the_diffable_view_keeps_what_a_timer_is_and_drops_when_it_fires() {
    // Act
    let observation = Observation::from(&table());
    let diffable = observation
        .in_view(View::Diffable)
        .expect("the facet survives the diffable view");

    // Assert: every timer is still there, and each has one field left.
    assert_eq!(keys_of(&diffable).len(), 3);
    let apt = field(&diffable, "apt-daily.timer");
    assert_eq!(keys_of(&apt), ["activates"]);
}

#[test]
fn an_absent_clock_is_volatile_too() {
    // Arrange: a timer that has never fired reports no last trigger, and reports one the
    // moment it does.
    let observation = Observation::from(&table());

    // Act
    let diffable = observation
        .in_view(View::Diffable)
        .expect("the facet survives");

    // Assert: if the absence were stable and the value volatile, the diffable view would
    // show `null` disappearing, which is the churn the annotation exists to remove.
    let never = field(&diffable, "never-ran.timer");
    assert_eq!(keys_of(&never), ["activates"]);
}

#[test]
fn the_complete_view_keeps_the_schedule() {
    // Act
    let observation = Observation::from(&table());

    // Assert
    let apt = field(&observation, "apt-daily.timer");
    assert_eq!(keys_of(&apt), ["activates", "last_trigger", "next_elapse"]);
    assert_eq!(
        field(&apt, "next_elapse").content(),
        &Content::Scalar(Scalar::Integer(1787250207868898))
    );
}

#[test]
fn presence_is_absent_when_the_host_does_not_run_systemd() {
    // Act & Assert
    assert_eq!(TimersCollector::reading(None).presence(), Presence::Absent);
}

#[test]
fn presence_is_present_when_systemd_is_on_the_host() {
    // Arrange: a tool the caller located, so this does not depend on the test machine.
    let timers = SystemctlTimers::using(
        rastro::collectors::canonical_tool::CanonicalTool::located_in("sh", &["/bin"])
            .expect("every unix has /bin/sh"),
    );

    // Act & Assert
    assert_eq!(
        TimersCollector::reading(Some(timers)).presence(),
        Presence::Present
    );
}

#[test]
fn an_empty_timer_list_is_a_present_facet_rather_than_an_absent_one() {
    // Act: a systemd box with no timers is a different statement from a box with no
    // systemd, and both are true things to say.
    let table = SystemctlTimers::parse("[]").expect("an empty list is well formed");

    // Assert
    assert!(table.is_empty());
}

#[test]
fn collect_fails_rather_than_reporting_an_empty_table_without_systemd() {
    // Act & Assert
    assert!(TimersCollector::reading(None).collect().is_err());
}

#[test]
fn parse_reads_a_timer_that_activates_null() {
    // Arrange: measured on a GitHub Actions runner, where `systemctl list-timers --output=json`
    // reports `"activates": null` for a timer that starts nothing systemd can name. The
    // development box wrote `""` for the same case, so the field was typed as a `String` with
    // `serde(default)` — which covers an *absent* field and not a null one. One such timer
    // failed the entire `timers` facet, and it failed it on every run of that host.
    let null_activates = r#"[
      {"unit":"orphan.timer","activates":null,"next":null,"last":null,"left":null,"passed":null}
    ]"#;

    // Act
    let table = SystemctlTimers::parse(null_activates)
        .expect("a null `activates` is a timer that starts nothing, not a broken document");

    // Assert: absent, null and empty all mean the same thing, and now the type admits it.
    assert_eq!(timer(&table, "orphan.timer").activates, None);
}

#[test]
fn parse_reads_a_timer_with_no_activates_field_at_all() {
    // Arrange: the third spelling of the same absence, which `serde(default)` always handled
    // and which must keep working.
    let absent = r#"[{"unit":"orphan.timer","next":null,"last":null,"left":null,"passed":null}]"#;

    // Act
    let table = SystemctlTimers::parse(absent)
        .expect("an absent `activates` is a timer that starts nothing");

    // Assert
    assert_eq!(timer(&table, "orphan.timer").activates, None);
}
