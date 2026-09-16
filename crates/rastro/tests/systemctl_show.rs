//! Reading what systemd will actually run, without needing a systemd to run it.
//!
//! Every fixture here is the development box's real output. The last group is the one
//! exception, and it was measured too: a throwaway unit written to `/run/systemd/system`,
//! shown, and removed without ever being started, because no unit on that box has two
//! `ExecStart=` lines or an argument containing a space, and both shapes decide how this
//! parser has to work.

use rastro::collectors::systemd::{ExecStart, ShownUnit, UnitName, systemctl_show};
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

fn shown() -> std::collections::BTreeMap<UnitName, ShownUnit> {
    systemctl_show::parse(SHOWN).expect("these fixtures are well formed")
}

fn starts_of(name: &str) -> Vec<ExecStart> {
    let unit = UnitName::new(name).expect("a legal unit name");

    shown()
        .get(&unit)
        .unwrap_or_else(|| panic!("expected {name:?} in the output"))
        .exec_start
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

/// Three units as systemd 257 on Debian 13 really printed them, asked for `Id`,
/// `ExecStartEx` and `Environment`.
///
/// **Measured, not composed.** Two throwaway units were written, shown and removed
/// without being started, because nothing on an ordinary box carries a value with a
/// space, a quote or a control character in it, and each of those decides how this
/// parser has to work.
///
/// What the fixture pins, in the order the entries appear:
///
/// - **`Environment=` is one line**, whatever the unit file spread over several lines,
///   and the entries on it are separated by spaces.
/// - **An entry is quoted only when it needs to be.** `SIMPLE=plain` is bare;
///   `"SPACED=two words"` is not, and the quotes wrap the whole `NAME=VALUE`, not the
///   value.
/// - **The value is split on the first `=` only**, which `EQUALS=a=b=c` is here to hold.
/// - **An empty value is legal** and is not the same as an absent variable.
/// - **A unit with no `Environment=` prints the key with nothing after it**, which is
///   how absence arrives, whereas a unit with no `EnvironmentFile=` prints no
///   `EnvironmentFiles=` line at all.
/// - **systemd C-escapes what it shows.** `NEWLINE=a\nb` is a real line feed on the
///   process, measured by reading `/usr/bin/env` out of the started unit, and
///   `BACKSLASH=a\\b` is one backslash. Recording the escaped spelling would put a value
///   in the document that was never in the process.
const SHOWN_WITH_ENVIRONMENT: &str = "\
ExecStartEx={ path=/bin/true ; argv[]=/bin/true ; flags= ; start_time=[n/a] ; \
stop_time=[n/a] ; pid=0 ; code=(null) ; status=0/0 }
Environment=SIMPLE=plain \"SPACED=two words\" \"QUOTED=has\\\"quote\" EMPTY= EQUALS=a=b=c
EnvironmentFiles=/etc/envtest.env (ignore_errors=no)
EnvironmentFiles=/etc/missing.env (ignore_errors=yes)
Id=envtest.service

Environment=\"NEWLINE=a\\nb\" \"TAB=a\\tb\" \"BACKSLASH=a\\\\b\" \"SINGLE=it's\" \
\"DOLLAR=\\$\\$HOME\" UNICODE=héllo
Id=edge.service

Environment=
Id=systemd-journald.service
";

fn environment_of(name: &str) -> Vec<(String, String)> {
    let unit = UnitName::new(name).expect("a legal unit name");

    systemctl_show::parse(SHOWN_WITH_ENVIRONMENT)
        .expect("these fixtures are well formed")
        .get(&unit)
        .unwrap_or_else(|| panic!("expected {name:?} in the output"))
        .environment
        .iter()
        .map(|(name, value)| (name.as_str().to_owned(), value.clone()))
        .collect()
}

#[test]
fn an_environment_entry_is_split_on_its_first_equals() {
    // Act
    let environment = environment_of("envtest.service");

    // Assert: sorted, because the model keys them by name and a diff needs one order.
    assert_eq!(
        environment,
        vec![
            ("EMPTY".to_owned(), String::new()),
            ("EQUALS".to_owned(), "a=b=c".to_owned()),
            ("QUOTED".to_owned(), "has\"quote".to_owned()),
            ("SIMPLE".to_owned(), "plain".to_owned()),
            ("SPACED".to_owned(), "two words".to_owned()),
        ]
    );
}

#[test]
fn a_shown_value_is_unescaped_to_what_the_process_actually_gets() {
    // Act
    let environment = environment_of("edge.service");

    // Assert: the escaped spelling is systemd's wire form, not the value. `a\nb` on the
    // wire is three characters on the process, and a fingerprint recording the wire form
    // would name a value that was never in anything's environment.
    assert_eq!(
        environment,
        vec![
            ("BACKSLASH".to_owned(), "a\\b".to_owned()),
            ("DOLLAR".to_owned(), "$$HOME".to_owned()),
            ("NEWLINE".to_owned(), "a\nb".to_owned()),
            ("SINGLE".to_owned(), "it's".to_owned()),
            ("TAB".to_owned(), "a\tb".to_owned()),
            ("UNICODE".to_owned(), "héllo".to_owned()),
        ]
    );
}

#[test]
fn a_unit_that_sets_no_environment_reports_an_empty_one() {
    // Act & Assert: systemd prints the key with nothing after it, which is absence and
    // not a parse failure.
    assert!(environment_of("systemd-journald.service").is_empty());
}

#[test]
fn an_environment_entry_with_no_equals_is_refused() {
    // Arrange: not a shape systemd produces, which is exactly why it is refused rather
    // than skipped — reaching it means this parser has misread the line, and a silently
    // dropped variable is a variable the diff will never mention.
    let malformed = "Environment=NAMEONLY\nId=broken.service\n";

    // Act
    let result = systemctl_show::parse(malformed);

    // Assert
    let failure = result.expect_err("an entry with no `=` cannot be a variable");
    assert!(
        failure.to_string().contains("NAMEONLY"),
        "the operator needs to know which entry, got: {failure}"
    );
}

fn environment_files_of(name: &str) -> Vec<(String, bool)> {
    let unit = UnitName::new(name).expect("a legal unit name");

    systemctl_show::parse(SHOWN_WITH_ENVIRONMENT)
        .expect("these fixtures are well formed")
        .get(&unit)
        .unwrap_or_else(|| panic!("expected {name:?} in the output"))
        .environment_files
        .iter()
        .map(|file| (file.path.as_str().to_owned(), file.ignore_errors))
        .collect()
}

#[test]
fn an_environment_file_carries_its_path_and_whether_systemd_tolerates_its_absence() {
    // Act
    let files = environment_files_of("envtest.service");

    // Assert: `ignore_errors` is systemd's own word for the `-` prefix in
    // `EnvironmentFile=-/path`, kept as systemd spells it. The distinction is behaviour, not
    // bookkeeping: a required file that did not survive a migration stops the unit, and an
    // optional one is designed not to.
    assert_eq!(
        files,
        vec![
            ("/etc/envtest.env".to_owned(), false),
            ("/etc/missing.env".to_owned(), true),
        ]
    );
}

#[test]
fn environment_files_keep_systemds_order_because_a_later_file_overrides_an_earlier_one() {
    // Arrange: two files whose paths sort the other way round from the order the unit reads
    // them, so a parser that sorted would be caught.
    let shown = "\
EnvironmentFiles=/etc/zzz.env (ignore_errors=no)
EnvironmentFiles=/etc/aaa.env (ignore_errors=no)
Id=ordered.service
";
    let unit = UnitName::new("ordered.service").expect("a legal unit name");

    // Act
    let parsed = systemctl_show::parse(shown).expect("a well formed group");
    let paths: Vec<&str> = parsed[&unit]
        .environment_files
        .iter()
        .map(|file| file.path.as_str())
        .collect();

    // Assert: last one wins on a repeated variable, so the order *is* the meaning. Sorting
    // these would be the same mistake as sorting `/proc/mounts`.
    assert_eq!(paths, ["/etc/zzz.env", "/etc/aaa.env"]);
}

#[test]
fn a_unit_with_no_environment_file_prints_no_line_and_reports_none() {
    // Act & Assert: unlike `Environment=`, which prints an empty value, this property is
    // absent from the group entirely. Two different spellings of absence in one dump.
    assert!(environment_files_of("edge.service").is_empty());
}

#[test]
fn an_environment_file_whose_line_has_no_ignore_errors_is_refused() {
    // Arrange: not a shape systemd produces, so reaching it means this parser misread the
    // line, and a path recorded without knowing whether it is required is worse than none.
    let malformed = "EnvironmentFiles=/etc/bare.env\nId=broken.service\n";

    // Act
    let result = systemctl_show::parse(malformed);

    // Assert
    let failure = result.expect_err("a line with no `(ignore_errors=…)` cannot be read");
    assert!(
        failure.to_string().contains("/etc/bare.env"),
        "the operator needs to know which line, got: {failure}"
    );
}

#[test]
fn an_environment_file_path_may_contain_a_space() {
    // Arrange: systemd quotes nothing on this property, so the path runs up to the last
    // ` (ignore_errors=`. Splitting on the first space would truncate this one.
    let shown = "\
EnvironmentFiles=/etc/my app.env (ignore_errors=no)
Id=spaced.service
";
    let unit = UnitName::new("spaced.service").expect("a legal unit name");

    // Act
    let parsed = systemctl_show::parse(shown).expect("a well formed group");

    // Assert
    assert_eq!(
        parsed[&unit].environment_files[0].path.as_str(),
        "/etc/my app.env"
    );
}

/// The `Environment=` line of a one-unit group, parsed.
fn environment_line(value: &str) -> Result<Vec<(String, String)>, String> {
    let shown = format!("Environment={value}\nId=probe.service\n");

    match systemctl_show::parse(&shown) {
        Ok(parsed) => Ok(
            parsed[&UnitName::new("probe.service").expect("a legal unit name")]
                .environment
                .iter()
                .map(|(name, value)| (name.as_str().to_owned(), value.clone()))
                .collect(),
        ),
        Err(failure) => Err(failure.to_string()),
    }
}

fn one_value(value: &str) -> String {
    let parsed = environment_line(value).expect("a well formed line");
    assert_eq!(parsed.len(), 1, "expected one variable from {value:?}");

    parsed[0].1.clone()
}

#[test]
fn every_escape_systemds_cescape_emits_is_resolved() {
    // Arrange & Assert: the table is systemd's `cescape`, and a branch of it that no test
    // reaches is a branch nobody has checked against the thing it is copying. The measured
    // cases upstream cover `\"`, `\\` and `\$`; these are the rest of the table.
    for (escaped, expected) in [
        ("A=x\\ay", '\u{7}'),
        ("A=x\\by", '\u{8}'),
        ("A=x\\fy", '\u{c}'),
        ("A=x\\ny", '\n'),
        ("A=x\\ry", '\r'),
        ("A=x\\ty", '\t'),
        ("A=x\\vy", '\u{b}'),
        ("A=x\\'y", '\''),
    ] {
        assert_eq!(
            one_value(escaped),
            format!("x{expected}y"),
            "escape in {escaped:?}"
        );
    }
}

#[test]
fn a_hex_escape_becomes_the_byte_it_names() {
    // Act & Assert: systemd writes `\xNN` for a byte it will not print.
    assert_eq!(one_value("A=x\\x41y"), "xAy");
}

#[test]
fn a_hex_escape_above_ascii_is_refused_rather_than_guessed_at() {
    // Arrange: one escaped byte is half a character in any multi-byte encoding, and the
    // value it belongs to is a Rust `String`. systemd leaves valid UTF-8 alone, so a high
    // byte here means the value was never text.

    // Act
    let failure = environment_line("A=x\\xffy").expect_err("a high byte is not text");

    // Assert
    assert!(
        failure.contains("0xff"),
        "the operator needs the byte named, got: {failure}"
    );
}

#[test]
fn a_hex_escape_that_is_not_two_hex_digits_is_refused() {
    // Act
    let failure = environment_line("A=x\\xzz").expect_err("`zz` is not hex");

    // Assert
    assert!(failure.contains("hex digits"), "got: {failure}");
}

#[test]
fn an_escape_outside_systemds_table_is_refused_rather_than_invented() {
    // Arrange: reaching this means rastro's table has fallen behind systemd's. Inventing a
    // character would put one in the document that is not in the process.

    // Act
    let failure = environment_line("A=x\\qy").expect_err("`\\q` is not in the table");

    // Assert
    assert!(failure.contains("cannot read"), "got: {failure}");
}

#[test]
fn a_line_ending_in_a_lone_backslash_is_refused() {
    // Act
    let failure = environment_line("A=x\\").expect_err("a trailing backslash escapes nothing");

    // Assert
    assert!(failure.contains("lone backslash"), "got: {failure}");
}

#[test]
fn an_environment_line_whose_quoting_does_not_close_is_refused() {
    // Arrange: where one variable ends cannot be told, so every entry after it is a guess.

    // Act
    let failure = environment_line("\"A=unterminated").expect_err("the quote never closes");

    // Assert
    assert!(failure.contains("quoting does not close"), "got: {failure}");
}

#[test]
fn a_name_repeated_on_one_environment_line_is_refused() {
    // Arrange: systemd has already merged the unit and its drop-ins by the time it prints
    // this, so two entries sharing a name means the line was split wrongly. Quietly keeping
    // one would hide the misreading behind a plausible answer.

    // Act
    let failure = environment_line("A=first A=second").expect_err("systemd prints no repeat");

    // Assert
    assert!(failure.contains("twice"), "got: {failure}");
}

#[test]
fn an_ignore_errors_value_systemd_does_not_print_is_refused() {
    // Arrange: the property is a boolean in systemd's own output, so anything else means
    // the line was misread, and a path recorded without knowing whether the unit needs it
    // is worse than no path.
    let shown = "EnvironmentFiles=/etc/x.env (ignore_errors=maybe)\nId=probe.service\n";

    // Act
    let failure = systemctl_show::parse(shown).expect_err("`maybe` is not a boolean");

    // Assert
    assert!(failure.to_string().contains("maybe"), "got: {failure}");
}

#[test]
fn the_names_a_unit_unsets_are_read_from_their_own_property() {
    // Arrange: measured against systemd 257. `UnsetEnvironment=` is one space-separated
    // line, and it is the last step of building a service's environment — a name here does
    // not reach the process whatever set it, including an `Environment=` on the same unit.
    let shown = "\
Environment=TOKEN=secret KEEP=yes
UnsetEnvironment=TOKEN
Id=probe.service
";
    let unit = UnitName::new("probe.service").expect("a legal unit name");

    // Act
    let parsed = systemctl_show::parse(shown).expect("a well formed group");

    // Assert: both facts survive, because removing the `Environment=` line and adding an
    // `UnsetEnvironment=` reach the same process environment by different edits, and only a
    // document carrying both can say which one happened.
    let shown_unit = &parsed[&unit];
    assert_eq!(
        shown_unit
            .unset_environment
            .iter()
            .map(|name| name.as_str())
            .collect::<Vec<&str>>(),
        ["TOKEN"]
    );
    assert_eq!(shown_unit.environment.len(), 2, "the declarations are kept");
}

#[test]
fn a_unit_that_unsets_nothing_reports_an_empty_list() {
    // Act & Assert: systemd prints the key with nothing after it, as it does for
    // `Environment=`, so absence is an empty list rather than a parse failure.
    let shown = "UnsetEnvironment=\nId=probe.service\n";
    let unit = UnitName::new("probe.service").expect("a legal unit name");

    assert!(
        systemctl_show::parse(shown).expect("a well formed group")[&unit]
            .unset_environment
            .is_empty()
    );
}
