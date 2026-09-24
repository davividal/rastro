//! Reading systemd's units, without needing a systemd to read them from.
//!
//! Every fixture row below is a real row from `systemctl --output=json` on the
//! development box, copied verbatim. That matters more here than usual: the shapes
//! that drove the design are all things systemd does and none of them are what a first
//! guess would invent.

mod support;

use rastro::collectors::systemd::EnvironmentFile;
use rastro::collectors::units::{
    EnvironmentReading, EnvironmentSource, LoadState, Systemctl, Unit, UnitFileState, UnitName,
    UnitRegistry, UnitsCollector,
};
use rastro_collector::{Collector, EnvironmentVariableName, Presence};
use rastro_fingerprint::{Content, Observation, Presentation, Scalar, View};
use support::observation::{field, is_null, items_of, keys_of};

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

fn text(observation: &Observation) -> String {
    match observation.content() {
        Content::Scalar(Scalar::Text(value)) => value.clone(),
        other => panic!("expected text, got {other:?}"),
    }
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
    assert_eq!(
        keys_of(&template),
        [
            "environment",
            "environment_files",
            "exec_start",
            "file",
            "runtime",
            "unset_environment"
        ]
    );
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

#[test]
fn a_units_environment_values_are_withheld_and_its_names_are_not() {
    // Arrange: a unit carrying the shape a real deployment has, one credential and one
    // setting that is not.
    let mut environment = std::collections::BTreeMap::new();
    environment.insert(
        EnvironmentVariableName::new("DATABASE_URL").expect("a legal name"),
        "postgres://app:hunter2@localhost/app".to_owned(),
    );
    environment.insert(
        EnvironmentVariableName::new("RUST_LOG").expect("a legal name"),
        "info".to_owned(),
    );
    let unit = Unit {
        file: None,
        runtime: None,
        exec_start: Vec::new(),
        environment,
        environment_files: Vec::new(),
        unset_environment: Vec::new(),
    };

    // Act
    let rendered = Observation::from(&unit)
        .in_view(Presentation::complete())
        .expect("nothing here is volatile");
    let shown = field(&rendered, "environment");

    // Assert: the name is the migration finding and is not a secret, so it stays legible.
    // The value is where the password is, so it does not.
    assert_eq!(keys_of(&shown), ["DATABASE_URL", "RUST_LOG"]);
    for name in ["DATABASE_URL", "RUST_LOG"] {
        assert!(
            text(&field(&shown, name)).starts_with("redacted:sha256+xxh3:"),
            "{name} reached the document as it stands"
        );
    }
}

#[test]
fn a_units_environment_value_is_readable_under_raw() {
    // Arrange
    let mut environment = std::collections::BTreeMap::new();
    environment.insert(
        EnvironmentVariableName::new("RUST_LOG").expect("a legal name"),
        "info".to_owned(),
    );
    let unit = Unit {
        file: None,
        runtime: None,
        exec_start: Vec::new(),
        environment,
        environment_files: Vec::new(),
        unset_environment: Vec::new(),
    };

    // Act
    let rendered = Observation::from(&unit)
        .in_view(Presentation::complete().raw())
        .expect("nothing here is volatile");

    // Assert: the annotation is a classification, and what to do about it is decided at
    // render time. An operator who opted out reads the value.
    assert_eq!(
        text(&field(&field(&rendered, "environment"), "RUST_LOG")),
        "info"
    );
}

#[test]
fn a_unit_configured_only_through_a_file_declares_no_variables_and_still_names_the_file() {
    // Arrange: the shape that makes the two fields need reading together. `Environment=` is
    // empty because systemd opens the file at exec time, and the unit runs with whatever is
    // in it.
    let unit = Unit {
        file: None,
        runtime: None,
        exec_start: Vec::new(),
        environment: std::collections::BTreeMap::new(),
        environment_files: vec![
            source_read(
                "/etc/myapp.env",
                false,
                [("DATABASE_URL", "postgres://x")],
                0,
            ),
            source_read("/etc/myapp.local.env", true, [], 0),
        ],
        unset_environment: Vec::new(),
    };

    // Act
    let rendered = Observation::from(&unit)
        .in_view(Presentation::complete())
        .expect("nothing here is volatile");
    let files = items_of(&field(&rendered, "environment_files"));

    // Assert: an empty `environment` beside a populated `environment_files` is exactly the
    // case a reader would otherwise take for "this unit sets nothing".
    assert!(keys_of(&field(&rendered, "environment")).is_empty());
    assert_eq!(files.len(), 2);
    assert_eq!(text(&field(&files[0], "path")), "/etc/myapp.env");
    assert_eq!(
        field(&files[0], "ignore_errors").content(),
        &Content::Scalar(Scalar::Boolean(false)),
        "the unit requires this one, so its absence after a migration stops the service"
    );
    assert_eq!(
        field(&files[1], "ignore_errors").content(),
        &Content::Scalar(Scalar::Boolean(true))
    );
}

#[test]
fn an_environment_file_path_is_not_withheld() {
    // Arrange: the path is the migration finding and is not a credential. What the file
    // holds is not read at all, so there is nothing here to redact.
    let unit = Unit {
        file: None,
        runtime: None,
        exec_start: Vec::new(),
        environment: std::collections::BTreeMap::new(),
        environment_files: vec![source_read("/etc/myapp.env", false, [], 0)],
        unset_environment: Vec::new(),
    };

    // Act
    let rendered = Observation::from(&unit)
        .in_view(Presentation::complete())
        .expect("nothing here is volatile");

    // Assert
    let files = items_of(&field(&rendered, "environment_files"));
    assert_eq!(text(&field(&files[0], "path")), "/etc/myapp.env");
}

#[test]
fn a_wildcard_records_the_pattern_and_the_file_it_matched_apart() {
    // Arrange: the pattern changes when somebody edits the unit, the matched file when
    // somebody drops one into the directory, so the two are recorded as separate facts. A
    // pattern that matched nothing has no file to name.
    let matched = EnvironmentSource {
        resolved: Some(
            rastro_collector::AbsolutePath::new("/etc/app.d/10-base.env", "unit environment file")
                .expect("an absolute path"),
        ),
        ..source_read("/etc/app.d/*.env", false, [], 0)
    };
    let unmatched = EnvironmentSource {
        declared: EnvironmentFile::new("/etc/none.d/*.env", true).expect("an absolute path"),
        resolved: None,
        reading: EnvironmentReading::Absent,
    };

    // Act
    let rendered = unit_with(vec![matched, unmatched]);

    // Assert
    let files = items_of(&field(&rendered, "environment_files"));
    assert_eq!(text(&field(&files[0], "declared")), "/etc/app.d/*.env");
    assert_eq!(text(&field(&files[0], "path")), "/etc/app.d/10-base.env");
    assert_eq!(text(&field(&files[1], "declared")), "/etc/none.d/*.env");
    assert!(
        is_null(&field(&files[1], "path")),
        "a pattern that matched nothing names no file"
    );
}

/// A declared file that was read, with the variables it set.
fn source_read<'a>(
    path: &str,
    ignore_errors: bool,
    variables: impl IntoIterator<Item = (&'a str, &'a str)>,
    ignored_lines: usize,
) -> EnvironmentSource {
    EnvironmentSource {
        declared: EnvironmentFile::new(path, ignore_errors).expect("an absolute path"),
        resolved: Some(
            rastro_collector::AbsolutePath::new(path, "unit environment file")
                .expect("an absolute path"),
        ),
        reading: EnvironmentReading::Read {
            variables: variables
                .into_iter()
                .map(|(name, value)| {
                    (
                        EnvironmentVariableName::new(name).expect("a legal name"),
                        value.to_owned(),
                    )
                })
                .collect(),
            ignored_lines,
        },
    }
}

fn unit_with(environment_files: Vec<EnvironmentSource>) -> Observation {
    let unit = Unit {
        file: None,
        runtime: None,
        exec_start: Vec::new(),
        environment: std::collections::BTreeMap::new(),
        environment_files,
        unset_environment: Vec::new(),
    };

    Observation::from(&unit)
        .in_view(Presentation::complete())
        .expect("nothing here is volatile")
}

#[test]
fn a_variable_read_out_of_an_environment_file_is_named_and_its_value_withheld() {
    // Act
    let rendered = unit_with(vec![source_read(
        "/etc/myapp.env",
        false,
        [("DATABASE_URL", "postgres://app:hunter2@localhost/app")],
        0,
    )]);
    let file = &items_of(&field(&rendered, "environment_files"))[0];

    // Assert: this is the whole point of the step. The name tells an operator the service
    // needs `DATABASE_URL` and the file must come across; the value is the credential and
    // does not leave the box in the document.
    assert_eq!(text(&field(file, "status")), "ok");
    assert_eq!(keys_of(&field(file, "variables")), ["DATABASE_URL"]);
    let value = text(&field(&field(file, "variables"), "DATABASE_URL"));
    assert!(
        value.starts_with("redacted:sha256+xxh3:"),
        "a credential reached the document as it stands: {value:?}"
    );
    assert!(!value.contains("hunter2"), "got {value:?}");
}

#[test]
fn a_file_the_unit_requires_and_that_is_not_there_is_absent_rather_than_a_failure() {
    // Act
    let rendered = unit_with(vec![EnvironmentSource {
        declared: EnvironmentFile::new("/etc/gone.env", false).expect("an absolute path"),
        resolved: Some(
            rastro_collector::AbsolutePath::new("/etc/gone.env", "unit environment file")
                .expect("an absolute path"),
        ),
        reading: EnvironmentReading::Absent,
    }]);
    let file = &items_of(&field(&rendered, "environment_files"))[0];

    // Assert: absence is state, and this is the loudest thing this facet can say. A unit
    // whose required environment file is missing will not start, so the pair of
    // `ignore_errors: false` and `status: absent` is a finding rather than a gap.
    assert_eq!(text(&field(file, "status")), "absent");
    assert_eq!(
        field(file, "ignore_errors").content(),
        &Content::Scalar(Scalar::Boolean(false))
    );
    assert_eq!(
        field(file, "variables").content(),
        &Content::Scalar(Scalar::Null)
    );
}

#[test]
fn a_file_that_would_not_open_counts_as_an_item_not_read_and_one_that_did_does_not() {
    // Arrange: the unit is still described either way; only the refused file is a gap.
    let refused = EnvironmentSource {
        declared: EnvironmentFile::new("/etc/secret.env", false).expect("an absolute path"),
        resolved: Some(
            rastro_collector::AbsolutePath::new("/etc/secret.env", "unit environment file")
                .expect("an absolute path"),
        ),
        reading: EnvironmentReading::Unreadable("Permission denied (os error 13)".to_owned()),
    };
    let read = source_read("/etc/app.env", false, [("A", "1")], 0);

    // Act
    let rendered = unit_with(vec![refused, read]);

    // Assert
    assert_eq!(rendered.incomplete_items(), 1);
}

#[test]
fn a_file_that_would_not_open_is_an_error_and_not_an_absence() {
    // Act
    let rendered = unit_with(vec![EnvironmentSource {
        declared: EnvironmentFile::new("/etc/secret.env", false).expect("an absolute path"),
        resolved: Some(
            rastro_collector::AbsolutePath::new("/etc/secret.env", "unit environment file")
                .expect("an absolute path"),
        ),
        reading: EnvironmentReading::Unreadable("Permission denied (os error 13)".to_owned()),
    }]);
    let file = &items_of(&field(&rendered, "environment_files"))[0];

    // Assert: the distinction the three-valued presence exists for, one level down. rastro
    // runs unprivileged often enough that "I was not allowed to look" is a routine answer,
    // and reporting it as `absent` would claim the file is gone.
    assert_eq!(text(&field(file, "status")), "error");
    assert!(text(&field(file, "error")).contains("Permission denied"));
    assert_eq!(
        field(file, "variables").content(),
        &Content::Scalar(Scalar::Null)
    );
}

#[test]
fn a_line_systemd_would_set_nothing_from_is_counted_in_the_document() {
    // Act
    let rendered = unit_with(vec![source_read("/etc/myapp.env", false, [], 2)]);
    let file = &items_of(&field(&rendered, "environment_files"))[0];

    // Assert: `export FOO=bar` is the line an operator writes and systemd ignores, so a
    // file can look right and set nothing. The count is what surfaces that.
    assert_eq!(
        field(file, "ignored_lines").content(),
        &Content::Scalar(Scalar::Integer(2))
    );
}

#[test]
fn a_variable_the_unit_unsets_is_named_beside_the_one_that_declares_it() {
    // Arrange: measured against systemd 257 — a unit declaring `Environment=TOKEN=secret`
    // beside `UnsetEnvironment=TOKEN` starts a process with no `TOKEN` at all.
    let mut environment = std::collections::BTreeMap::new();
    environment.insert(
        EnvironmentVariableName::new("TOKEN").expect("a legal name"),
        "secret".to_owned(),
    );
    let unit = Unit {
        file: None,
        runtime: None,
        exec_start: Vec::new(),
        environment,
        environment_files: Vec::new(),
        unset_environment: vec![EnvironmentVariableName::new("TOKEN").expect("a legal name")],
    };

    // Act
    let rendered = Observation::from(&unit)
        .in_view(Presentation::complete())
        .expect("nothing here is volatile");

    // Assert: both facts are kept. Reporting only the declaration would claim the service
    // has a variable it never receives; dropping the declaration would hide that removing
    // the `UnsetEnvironment=` line would give it one.
    assert_eq!(keys_of(&field(&rendered, "environment")), ["TOKEN"]);
    assert_eq!(
        items_of(&field(&rendered, "unset_environment"))
            .iter()
            .map(text)
            .collect::<Vec<String>>(),
        ["TOKEN"]
    );
}

#[test]
fn an_environment_file_entry_always_carries_the_same_keys() {
    // Arrange: the output format is the contract, and a key that appears only sometimes is
    // awkward for every consumer. Three readings that differ as much as they can — read,
    // absent, and a pattern that matched nothing — must still render one shape.
    let read = source_read("/etc/a.env", false, [("A", "1")], 0);
    let absent = EnvironmentSource {
        declared: EnvironmentFile::new("/etc/gone.env", true).expect("an absolute path"),
        resolved: Some(
            rastro_collector::AbsolutePath::new("/etc/gone.env", "unit environment file")
                .expect("an absolute path"),
        ),
        reading: EnvironmentReading::Absent,
    };
    let unmatched = EnvironmentSource {
        declared: EnvironmentFile::new("/etc/none.d/*.env", false).expect("an absolute path"),
        resolved: None,
        reading: EnvironmentReading::Unreadable("refused".to_owned()),
    };

    // Act
    let rendered = unit_with(vec![read, absent, unmatched]);

    // Assert
    let expected = [
        "declared",
        "error",
        "ignore_errors",
        "ignored_lines",
        "path",
        "status",
        "variables",
    ];
    for entry in items_of(&field(&rendered, "environment_files")) {
        assert_eq!(keys_of(&entry), expected, "got {entry:?}");
    }
}
