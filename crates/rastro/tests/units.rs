//! Reading systemd's units, without needing a systemd to read them from.
//!
//! Every fixture row below is a real row from `systemctl --output=json` on the
//! development box, copied verbatim. That matters more here than usual: the shapes
//! that drove the design are all things systemd does and none of them are what a first
//! guess would invent.

use rastro::collectors::units::{
    LoadState, Systemctl, Unit, UnitFileState, UnitName, UnitRegistry, UnitsCollector,
};
use rastro_collector::{Collector, Presence};
use rastro_fingerprint::{Content, Observation, Scalar, View};

/// Real rows, covering an enabled service, a masked one, an alias, a template, a
/// runtime-enabled unit and the transient scope of a login session.
const UNIT_FILES: &str = r#"[
  {"unit_file":"ssh.service","state":"enabled","preset":"enabled"},
  {"unit_file":"sshd.service","state":"alias","preset":null},
  {"unit_file":"hwclock.service","state":"masked","preset":"enabled"},
  {"unit_file":"autovt@.service","state":"alias","preset":null},
  {"unit_file":"systemd-remount-fs.service","state":"enabled-runtime","preset":"enabled"},
  {"unit_file":"proc-sys-fs-binfmt_misc.mount","state":"disabled","preset":"disabled"},
  {"unit_file":"session-783.scope","state":"transient","preset":null}
]"#;

/// Real rows, covering a running daemon, a `not-found` reference, a slice systemd made
/// itself, and a login session's scope.
const UNITS: &str = r#"[
  {"unit":"ssh.service","load":"loaded","active":"active","sub":"running","description":"OpenBSD Secure Shell server"},
  {"unit":"NetworkManager.service","load":"not-found","active":"inactive","sub":"dead","description":"NetworkManager.service"},
  {"unit":"-.slice","load":"loaded","active":"active","sub":"active","description":"Root Slice"},
  {"unit":"session-783.scope","load":"loaded","active":"active","sub":"running","description":"Session 783 of User vagrant"},
  {"unit":"dev-disk-by\\x2ddiskseq-1.device","load":"loaded","active":"active","sub":"plugged","description":"HARDDISK"}
]"#;

/// Real groups, as `systemctl show -p Id -p ExecStartEx --no-pager -- <units>` prints
/// them for the same units: a daemon with a command, and a slice with none.
const SHOWN: &str = "\
ExecStartEx={ path=/usr/sbin/sshd ; argv[]=/usr/sbin/sshd -D $SSHD_OPTS ; flags= ; \
start_time=[n/a] ; stop_time=[n/a] ; pid=0 ; code=(null) ; status=0/0 }
Id=ssh.service

Id=-.slice
";

fn joined() -> UnitRegistry {
    Systemctl::join(UNIT_FILES, UNITS, SHOWN).expect("these fixtures are well formed")
}

fn unit(registry: &UnitRegistry, name: &str) -> Unit {
    registry
        .units()
        .get(&UnitName::new(name).expect("a legal unit name"))
        .unwrap_or_else(|| panic!("expected a {name:?} unit"))
        .clone()
}

fn object_of(observation: &Observation) -> Vec<(String, Observation)> {
    match observation.content() {
        Content::Object(entries) => entries
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
        other => panic!("expected an object, got {other:?}"),
    }
}

fn field(observation: &Observation, name: &str) -> Observation {
    object_of(observation)
        .into_iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value)
        .unwrap_or_else(|| panic!("expected a {name:?} field"))
}

fn text(observation: &Observation) -> String {
    match observation.content() {
        Content::Scalar(Scalar::Text(value)) => value.clone(),
        other => panic!("expected text, got {other:?}"),
    }
}

fn keys_of(observation: &Observation) -> Vec<String> {
    object_of(observation)
        .into_iter()
        .map(|(key, _)| key)
        .collect()
}

#[test]
fn join_reads_the_enablement_of_an_installed_unit() {
    // Act
    let ssh = unit(&joined(), "ssh.service");

    // Assert: the field `design.md` names the field research for.
    let file = ssh.file.expect("ssh.service has a unit file");
    assert_eq!(
        file.state,
        UnitFileState::new("enabled").expect("a legal state")
    );
    assert_eq!(
        file.preset.map(|preset| preset.as_str().to_owned()),
        Some("enabled".to_owned())
    );
}

#[test]
fn join_records_an_absent_preset_as_absent() {
    // Act: 189 of the 262 unit files on the development box have no preset.
    let alias = unit(&joined(), "sshd.service");

    // Assert
    assert_eq!(alias.file.expect("a unit file").preset, None);
}

#[test]
fn join_reads_the_runtime_state_of_a_loaded_unit() {
    // Act
    let ssh = unit(&joined(), "ssh.service");

    // Assert
    let runtime = ssh.runtime.expect("ssh.service is loaded");
    assert_eq!(runtime.load, LoadState::new("loaded").expect("legal"));
    assert_eq!(runtime.active.as_str(), "active");
    assert_eq!(runtime.sub.as_str(), "running");
    assert_eq!(
        runtime
            .description
            .map(|description| description.as_str().to_owned()),
        Some("OpenBSD Secure Shell server".to_owned())
    );
}

#[test]
fn join_keeps_a_unit_that_has_a_file_but_is_not_loaded() {
    // Act: a template is instantiated rather than loaded, so it never appears in
    // `list-units`. 106 units on the development box are in this position.
    let template = unit(&joined(), "autovt@.service");

    // Assert: dropping these would make the facet quietly incomplete.
    assert!(template.file.is_some());
    assert_eq!(template.runtime, None);
}

#[test]
fn join_keeps_a_unit_that_is_loaded_with_no_file_behind_it() {
    // Act: something in the dependency graph references NetworkManager and the box has
    // never had it installed. 23 units on the development box are `not-found`.
    let dangling = unit(&joined(), "NetworkManager.service");

    // Assert: a facet listing only unit files would say nothing about a dangling
    // reference like this.
    assert_eq!(dangling.file, None);
    let runtime = dangling.runtime.expect("it is loaded, after a fashion");
    assert_eq!(runtime.load, LoadState::new("not-found").expect("legal"));
}

#[test]
fn join_gives_a_not_found_unit_the_description_systemd_reports_for_it() {
    // Act: systemd substitutes the unit's own name, which is not what a first guess
    // expects and is why this is pinned.
    let dangling = unit(&joined(), "NetworkManager.service");

    // Assert
    assert_eq!(
        dangling
            .runtime
            .expect("loaded")
            .description
            .map(|description| description.as_str().to_owned()),
        Some("NetworkManager.service".to_owned())
    );
}

#[test]
fn join_sees_both_sides_of_a_unit_that_has_them() {
    // Act
    let registry = joined();

    // Assert: seven unit files and five loaded units, overlapping on two names, is ten
    // distinct units.
    assert_eq!(registry.len(), 10);
    let both = registry
        .units()
        .values()
        .filter(|unit| unit.file.is_some() && unit.runtime.is_some())
        .count();
    assert_eq!(both, 2);
}

#[test]
fn join_leaves_systemd_escaping_in_a_unit_name_alone() {
    // Act: decoding `\x2d` to `-` would destroy the only thing separating a literal
    // hyphen from a path separator, and the escaped form is the real name.
    let registry = joined();

    // Assert
    assert!(
        registry
            .units()
            .keys()
            .any(|name| name.as_str() == r"dev-disk-by\x2ddiskseq-1.device"),
        "the escaped name must survive, got {:?}",
        registry.units().keys().collect::<Vec<&UnitName>>()
    );
}

#[test]
fn join_orders_units_by_name() {
    // Act
    let observation = Observation::from(&joined());

    // Assert: the order systemd happened to list them in never reaches the document.
    let mut sorted = keys_of(&observation);
    sorted.sort();
    assert_eq!(keys_of(&observation), sorted);
}

#[test]
fn join_refuses_a_unit_reported_twice() {
    // Arrange: systemd enforces one unit per name, so a repeat means rastro misread the
    // output, and keeping the last of two would drop a unit.
    let repeated = r#"[
      {"unit_file":"ssh.service","state":"enabled","preset":"enabled"},
      {"unit_file":"ssh.service","state":"disabled","preset":"enabled"}
    ]"#;

    // Act
    let result = Systemctl::join(repeated, "[]", "");

    // Assert
    let failure = result.expect_err("a repeated unit must not be silently dropped");
    assert!(
        failure.to_string().contains("ssh.service"),
        "the message must name the unit, got: {failure}"
    );
}

#[test]
fn join_refuses_output_that_is_not_json() {
    // Act: the tabular output, which is what an older systemd would give.
    let result = Systemctl::join(
        "UNIT FILE STATE PRESET\nssh.service enabled enabled\n",
        "[]",
        "",
    );

    // Assert
    let failure = result.expect_err("a table is not JSON");
    assert!(
        failure.to_string().contains("list-unit-files"),
        "the message must name the subcommand, got: {failure}"
    );
}

#[test]
fn a_login_sessions_scope_is_marked_volatile() {
    // Arrange: this is the one piece of churn in the facet that moves a *key*. systemd
    // numbers a session's scope with a counter that rises on every login, and every ssh
    // connection to the development box created a new one: 779, 783, 785.
    let observation = Observation::from(&joined());

    // Act
    let diffable = observation
        .in_view(View::Diffable)
        .expect("the facet survives the diffable view");

    // Assert: the key is gone entirely, which is the only way to drop churn that lives
    // in a name rather than in a value.
    assert!(
        !keys_of(&diffable).contains(&"session-783.scope".to_owned()),
        "a login session's scope must not reach the diffable view, got {:?}",
        keys_of(&diffable)
    );
    assert!(
        keys_of(&observation).contains(&"session-783.scope".to_owned()),
        "it must still be in the complete view"
    );
}

#[test]
fn a_unit_whose_name_embeds_a_uid_is_not_treated_as_churn() {
    // Arrange: `user@1000.service` and `user-1000.slice` embed a uid rather than a
    // counter, and a uid appearing is a real change worth seeing.
    let files = r#"[
      {"unit_file":"user@1000.service","state":"static","preset":null},
      {"unit_file":"user-1000.slice","state":"static","preset":null},
      {"unit_file":"session-1.scope","state":"transient","preset":null}
    ]"#;

    // Act
    let observation = Observation::from(&Systemctl::join(files, "[]", "").expect("well formed"));
    let diffable = observation
        .in_view(View::Diffable)
        .expect("the facet survives");

    // Assert
    assert_eq!(keys_of(&diffable), ["user-1000.slice", "user@1000.service"]);
}

#[test]
fn a_scope_that_is_not_a_numbered_session_is_kept() {
    // Arrange: `init.scope` is a real, permanent scope, and only the counter-bearing
    // ones are churn.
    let files = r#"[{"unit_file":"init.scope","state":"static","preset":null}]"#;

    // Act
    let observation = Observation::from(&Systemctl::join(files, "[]", "").expect("well formed"));
    let diffable = observation
        .in_view(View::Diffable)
        .expect("the facet survives");

    // Assert
    assert_eq!(keys_of(&diffable), ["init.scope"]);
}

#[test]
fn a_unit_renders_both_sides_with_a_null_for_the_missing_one() {
    // Act
    let observation = Observation::from(&joined());
    let template = field(&observation, "autovt@.service");

    // Assert: a key that is sometimes present and sometimes missing is awkward for
    // every consumer, so both sides are always there and one is null.
    assert_eq!(keys_of(&template), ["exec_start", "file", "runtime"]);
    assert_eq!(
        field(&template, "runtime").content(),
        &Content::Scalar(Scalar::Null)
    );
    assert_eq!(text(&field(&field(&template, "file"), "state")), "alias");
}

#[test]
fn presence_is_present_when_systemd_is_on_the_host() {
    // Arrange: a tool the caller located, so this does not depend on the machine
    // running the test.
    let systemctl = Systemctl::using(
        rastro::collectors::canonical_tool::CanonicalTool::located_in("sh", &["/bin"])
            .expect("every unix has /bin/sh"),
    );

    // Act & Assert
    assert_eq!(
        UnitsCollector::reading(Some(systemctl)).presence(),
        Presence::Present
    );
}

#[test]
fn presence_is_absent_when_the_host_does_not_run_systemd() {
    // Act & Assert: unlike the packages collector, `absent` is exact here. There is no
    // second init rastro might have shipped a source for and did not: a box either runs
    // systemd or it has no systemd units.
    assert_eq!(UnitsCollector::reading(None).presence(), Presence::Absent);
}

#[test]
fn collect_fails_rather_than_reporting_an_empty_registry_without_systemd() {
    // Act: `presence` would have caught this first, so reaching `collect` is a
    // programming error rather than a host state.
    let result = UnitsCollector::reading(None).collect();

    // Assert: an empty registry passed off as the truth is the one thing that must not
    // happen.
    assert!(result.is_err());
}

#[test]
fn join_records_what_a_unit_starts() {
    // Act
    let ssh = unit(&joined(), "ssh.service");

    // Assert: the field that turns "enabled and active" into "and this is the binary it
    // amounts to, with these flags".
    assert_eq!(ssh.exec_start.len(), 1);
    assert_eq!(ssh.exec_start[0].executable.as_str(), "/usr/sbin/sshd");
    assert_eq!(
        ssh.exec_start[0].argv.as_str(),
        "/usr/sbin/sshd -D $SSHD_OPTS"
    );
}

#[test]
fn join_gives_a_unit_that_starts_nothing_an_empty_command_list() {
    // Act: the root slice is loaded and active and runs no process at all.
    let slice = unit(&joined(), "-.slice");

    // Assert
    assert!(slice.exec_start.is_empty());
}

#[test]
fn join_leaves_a_unit_systemd_did_not_resolve_starting_nothing() {
    // Act: a template has a file, is never loaded, and so was never shown.
    let template = unit(&joined(), "autovt@.service");

    // Assert: empty rather than a guess read out of the unit file, because resolving a
    // file into an effective command is systemd's job and rastro does not do it twice.
    assert!(template.exec_start.is_empty());
}

#[test]
fn what_a_unit_starts_survives_into_the_diffable_view() {
    // Act
    let observation = Observation::from(&joined());
    let diffable = observation
        .in_view(View::Diffable)
        .expect("the facet survives");

    // Assert: a resolved command is stable across two runs of an unchanged box, unlike the
    // pid and exit status systemd prints beside it, which this facet never reads.
    let ssh = field(&diffable, "ssh.service");
    let started = match field(&ssh, "exec_start").content() {
        Content::List(items) => items.clone(),
        other => panic!("expected a list, got {other:?}"),
    };
    assert_eq!(text(&field(&started[0], "executable")), "/usr/sbin/sshd");
}
