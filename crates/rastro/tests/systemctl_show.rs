//! Reading what systemd will actually run, without needing a systemd to run it.
//!
//! Every fixture here is the development box's real output. The last group is the one
//! exception, and it was measured too: a throwaway unit written to `/run/systemd/system`,
//! shown, and removed without ever being started, because no unit on that box has two
//! `ExecStart=` lines or an argument containing a space, and both shapes decide how this
//! parser has to work.

use rastro::collectors::systemd::{ExecStart, UnitName, systemctl_show};
use rastro_collector::{Content, Observation, Scalar};

/// Four units as `systemctl show <units> -p Id -p ExecStartEx --no-pager` prints them.
///
/// **The properties come back in systemd's order, not the order they were asked for**,
/// which is why `ExecStartEx` sits above `Id` here. A parser that read the group
/// positionally would work on this box and break on the first systemd that reorders them.
const SHOWN: &str = "\
ExecStartEx={ path=/usr/local/bin/cadvisor ; argv[]=/usr/local/bin/cadvisor \
--listen_ip=0.0.0.0 --port=8080 ; flags= ; start_time=[n/a] ; stop_time=[n/a] ; pid=0 ; \
code=(null) ; status=0/0 }
Id=cadvisor.service

Id=dbus.socket

ExecStartEx={ path=systemd-tmpfiles ; argv[]=systemd-tmpfiles --prefix=/dev --create \
--boot ; flags= ; start_time=[n/a] ; stop_time=[n/a] ; pid=0 ; code=(null) ; status=0/0 }
Id=systemd-tmpfiles-setup-dev.service

ExecStartEx={ path=/bin/echo ; argv[]=/bin/echo first ; flags= ; start_time=[n/a] ; \
stop_time=[n/a] ; pid=0 ; code=(null) ; status=0/0 }
ExecStartEx={ path=/bin/echo ; argv[]=/bin/echo --flag=a b second ; flags= ; \
start_time=[n/a] ; stop_time=[n/a] ; pid=0 ; code=(null) ; status=0/0 }
Id=rastro-probe.service
";

fn shown() -> std::collections::BTreeMap<UnitName, Vec<ExecStart>> {
    systemctl_show::parse(SHOWN).expect("these fixtures are well formed")
}

fn starts_of(name: &str) -> Vec<ExecStart> {
    let unit = UnitName::new(name).expect("a legal unit name");

    shown()
        .get(&unit)
        .unwrap_or_else(|| panic!("expected {name:?} in the output"))
        .clone()
}

fn text(observation: &Observation) -> String {
    match observation.content() {
        Content::Scalar(Scalar::Text(value)) => value.clone(),
        other => panic!("expected text, got {other:?}"),
    }
}

fn field(observation: &Observation, name: &str) -> Observation {
    match observation.content() {
        Content::Object(entries) => entries
            .get(name)
            .unwrap_or_else(|| panic!("expected a {name:?} field"))
            .clone(),
        other => panic!("expected an object, got {other:?}"),
    }
}

#[test]
fn parse_reads_the_command_a_unit_starts() {
    // Act
    let starts = starts_of("cadvisor.service");

    // Assert
    assert_eq!(starts.len(), 1);
    assert_eq!(starts[0].executable.as_str(), "/usr/local/bin/cadvisor");
    assert_eq!(
        starts[0].argv.as_str(),
        "/usr/local/bin/cadvisor --listen_ip=0.0.0.0 --port=8080"
    );
}

#[test]
fn parse_finds_every_unit_that_was_shown() {
    // Act
    let shown = shown();

    // Assert: including the one with nothing to start, because a unit systemd knows and
    // rastro dropped is the kind of quiet incompleteness this project does not accept.
    assert_eq!(shown.len(), 4);
}

#[test]
fn parse_gives_a_unit_with_nothing_to_start_no_exec_start() {
    // Act
    let starts = starts_of("dbus.socket");

    // Assert: systemd prints the `Id=` line and no `ExecStartEx=` line at all for a unit
    // that starts nothing, and an empty list is what that looks like.
    assert!(starts.is_empty());
}

#[test]
fn parse_keeps_an_executable_systemd_will_resolve_itself() {
    // Act
    let starts = starts_of("systemd-tmpfiles-setup-dev.service");

    // Assert: `path=systemd-tmpfiles` is not absolute, and it is a real unit shipped by
    // Debian 12. systemd resolves a bare name against its own compiled-in list, so
    // refusing it as "not an absolute path" would turn a working unit into a failure.
    assert_eq!(starts[0].executable.as_str(), "systemd-tmpfiles");
}

#[test]
fn parse_keeps_every_exec_start_of_a_unit_that_has_several() {
    // Act
    let starts = starts_of("rastro-probe.service");

    // Assert: in the order systemd runs them, which is the order it printed them. Keeping
    // only the first would report a unit that does something other than what it does.
    assert_eq!(starts.len(), 2);
    assert_eq!(starts[0].argv.as_str(), "/bin/echo first");
    assert_eq!(starts[1].argv.as_str(), "/bin/echo --flag=a b second");
}

#[test]
fn parse_records_the_argument_vector_as_systemd_rendered_it() {
    // Arrange: the unit behind this line reads `ExecStart=/bin/echo --flag="a b" second`.
    let starts = starts_of("rastro-probe.service");

    // Act
    let argv = starts[1].argv.as_str();

    // Assert: **systemd does not preserve the quoting**, so `--flag=a b second` is three
    // whitespace-separated tokens standing for two arguments, and nothing in the output
    // says which. Splitting here would invent a structure the source cannot support, so
    // the line is kept whole and the splitting is left to a collector that knows the
    // program's flags well enough to be refuted by a bad split.
    assert_eq!(argv, "/bin/echo --flag=a b second");
}

#[test]
fn parse_refuses_a_group_that_names_no_unit() {
    // Arrange: an ExecStart rastro cannot attribute to a unit.
    let orphan = "ExecStartEx={ path=/bin/true ; argv[]=/bin/true ; flags= ; pid=0 }\n";

    // Act
    let refused = systemctl_show::parse(orphan);

    // Assert
    assert!(refused.is_err());
}

#[test]
fn parse_refuses_the_same_unit_twice() {
    // Arrange
    let repeated = "Id=ssh.service\n\nId=ssh.service\n";

    // Act
    let refused = systemctl_show::parse(repeated);

    // Assert: systemd enforces one unit per name, so a repeat means rastro misread the
    // output, and keeping the last of two would drop a unit silently.
    assert!(refused.is_err());
}

#[test]
fn parse_reads_nothing_from_nothing() {
    // Act
    let shown = systemctl_show::parse("").expect("silence is not a failure");

    // Assert
    assert!(shown.is_empty());
}

#[test]
fn an_exec_start_renders_as_its_executable_and_its_argument_vector() {
    // Act
    let observation = Observation::from(&starts_of("cadvisor.service")[0]);

    // Assert
    assert_eq!(
        text(&field(&observation, "executable")),
        "/usr/local/bin/cadvisor"
    );
    assert_eq!(
        text(&field(&observation, "argv")),
        "/usr/local/bin/cadvisor --listen_ip=0.0.0.0 --port=8080"
    );
}
