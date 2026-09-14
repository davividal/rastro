//! Reading an `EnvironmentFile=` the way systemd reads it.
//!
//! **Every expectation here was measured, not taken from `systemd.exec(5)`.** A unit with
//! `ExecStart=/usr/bin/env` was pointed at a probe file under systemd 257 on Debian 13 and
//! its output read back, because this format has several behaviours the manual does not
//! state and two that contradict what the same words mean in a unit file.
//!
//! rastro parses these files rather than asking systemd, and that is the nginx exception
//! being taken a second time: `systemctl show -p Environment` reports what a unit
//! *declares* and deliberately not what these files contribute, systemd opening them at
//! exec time. There is no non-mutating way to ask, so the format is read directly and the
//! divergence risk is carried here, in tests, rather than in a claim.

use rastro::collectors::units::environment_file_contents;

/// The variables a file's text sets, as `(name, value)` pairs in name order.
fn variables(text: &str) -> Vec<(String, String)> {
    environment_file_contents::parse(text)
        .variables
        .iter()
        .map(|(name, value)| (name.as_str().to_owned(), value.clone()))
        .collect()
}

fn value_of(text: &str) -> String {
    let parsed = variables(text);
    assert_eq!(parsed.len(), 1, "expected one variable from {text:?}");

    parsed[0].1.clone()
}

#[test]
fn a_comment_and_a_blank_line_set_nothing() {
    // Arrange: a `#` opens a comment only where the line starts, leading whitespace aside.
    let text = "# a comment\n\n  # indented comment\nSIMPLE=plain\n";

    // Act & Assert
    assert_eq!(
        variables(text),
        vec![("SIMPLE".to_owned(), "plain".to_owned())]
    );
}

#[test]
fn a_hash_after_a_value_is_part_of_the_value() {
    // Act & Assert: measured. The obvious reading, that `#` starts a comment anywhere, would
    // silently truncate a value.
    assert_eq!(
        value_of("HASH=value # not a comment?\n"),
        "value # not a comment?"
    );
}

#[test]
fn a_semicolon_is_not_a_comment_either() {
    // Act & Assert
    assert_eq!(value_of("SEMI=;notcomment\n"), ";notcomment");
}

#[test]
fn whitespace_around_the_equals_and_after_the_value_is_dropped() {
    // Act & Assert: `SPACED_EQ = around` sets `SPACED_EQ`, not `SPACED_EQ `, and a value is
    // not padded by however the file was aligned.
    assert_eq!(
        variables("  INDENTED=yes\nSPACED_EQ = around\nTRAILING=value   \nBLANKVAL=   \n"),
        vec![
            ("BLANKVAL".to_owned(), String::new()),
            ("INDENTED".to_owned(), "yes".to_owned()),
            ("SPACED_EQ".to_owned(), "around".to_owned()),
            ("TRAILING".to_owned(), "value".to_owned()),
        ]
    );
}

#[test]
fn a_value_is_split_on_the_first_equals_only() {
    // Act & Assert
    assert_eq!(value_of("EQUALS=a=b=c\n"), "a=b=c");
}

#[test]
fn both_quote_styles_are_stripped() {
    // Act & Assert: single quotes work here, which is one of the ways this format is not the
    // `Environment=` line of a unit file.
    assert_eq!(value_of("DQUOTED=\"two words\"\n"), "two words");
    assert_eq!(value_of("SQUOTED='two words'\n"), "two words");
}

#[test]
fn a_backslash_escapes_a_quote_and_itself_inside_double_quotes() {
    // Act & Assert
    assert_eq!(value_of("ESCAPED=\"has\\\"quote\"\n"), "has\"quote");
    assert_eq!(value_of("BACKSLASH=\"a\\\\b\"\n"), "a\\b");
}

#[test]
fn backslash_n_inside_double_quotes_is_two_characters_and_not_a_newline() {
    // Act & Assert: **this is the trap.** The same spelling in a unit file's `Environment=`
    // is a real line feed, measured; here it is a backslash and an `n`. A parser that reused
    // the unit-file escape table would put a character in the document that is not in the
    // process.
    assert_eq!(value_of("NEWLINE=\"a\\nb\"\n"), "a\\nb");
}

#[test]
fn an_unquoted_backslash_is_an_escape_and_disappears() {
    // Act & Assert: measured, and the opposite of the quoted case above. `a\b` unquoted is
    // `ab`; inside quotes it would have kept the backslash.
    assert_eq!(value_of("UNQ_BS=a\\b\n"), "ab");
}

#[test]
fn a_backslash_at_end_of_line_continues_onto_the_next() {
    // Act & Assert: the line-level behaviour, which matters more than any escape. Getting
    // this wrong swallows the following line, and the following line's *name* is something
    // the document shows in the clear.
    assert_eq!(value_of("CONTINUED=\"first \\\nsecond\"\n"), "first second");
}

#[test]
fn a_continuation_does_not_swallow_the_variable_after_it() {
    // Arrange: the regression the test above is really about.
    let text = "CONTINUED=\"first \\\nsecond\"\nAFTER=intact\n";

    // Act & Assert
    assert_eq!(
        variables(text),
        vec![
            ("AFTER".to_owned(), "intact".to_owned()),
            ("CONTINUED".to_owned(), "first second".to_owned()),
        ]
    );
}

#[test]
fn the_last_assignment_of_a_repeated_name_wins() {
    // Act & Assert: systemd's own rule, mirrored rather than refused. The cron collector
    // refuses a repeat because a crontab is genuinely ambiguous about what its jobs run
    // with; here the semantics are defined, so reporting anything else would be rastro
    // inventing a disagreement.
    assert_eq!(value_of("DUPE=first\nDUPE=second\n"), "second");
}

#[test]
fn a_line_systemd_would_ignore_is_counted_rather_than_guessed_at() {
    // Arrange: `export FOO=bar` is the one an operator actually writes. systemd drops it,
    // because the name would contain a space, so the variable is simply not set and the
    // file looks fine.
    let text = "export EXPORTED=maybe\nNOEQUALS\n=NONAME\nSIMPLE=plain\n";

    // Act
    let parsed = environment_file_contents::parse(text);

    // Assert: the count is what makes a misconfiguration visible. Reporting the variable as
    // set would be a lie, and saying nothing would hide a file whose author is wrong about
    // what it does.
    assert_eq!(parsed.variables.len(), 1);
    assert_eq!(parsed.ignored_lines, 3);
}

#[test]
fn a_last_line_without_a_newline_still_counts() {
    // Act & Assert
    assert_eq!(value_of("TRAILNL=x"), "x");
}

#[test]
fn an_empty_file_sets_nothing_and_ignores_nothing() {
    // Act
    let parsed = environment_file_contents::parse("");

    // Assert
    assert!(parsed.variables.is_empty());
    assert_eq!(parsed.ignored_lines, 0);
}

/// The probe file, byte for byte as systemd 257 was given it.
const PROBE: &str = r#"# a comment
SIMPLE=plain

  INDENTED=yes
export EXPORTED=maybe
DQUOTED="two words"
SQUOTED='two words'
ESCAPED="has\"quote"
BACKSLASH="a\\b"
NEWLINE="a\nb"
EQUALS=a=b=c
EMPTY=
SPACED_EQ = around
TRAILING=value   
CONTINUED="first \
second"
HASH=value # not a comment?
"#;

#[test]
fn the_whole_probe_file_reads_as_systemd_read_it() {
    // Arrange: the other tests in this file each isolate one rule. This one is the
    // measurement itself — the right-hand side is `/usr/bin/env` run as a unit with this
    // file as its `EnvironmentFile=`, with systemd's own additions filtered out. If this
    // parser and systemd ever part company, it should be here that it shows.
    let expected = vec![
        ("BACKSLASH", "a\\b"),
        ("CONTINUED", "first second"),
        ("DQUOTED", "two words"),
        ("EMPTY", ""),
        ("EQUALS", "a=b=c"),
        ("ESCAPED", "has\"quote"),
        ("HASH", "value # not a comment?"),
        ("INDENTED", "yes"),
        ("NEWLINE", "a\\nb"),
        ("SIMPLE", "plain"),
        ("SPACED_EQ", "around"),
        ("SQUOTED", "two words"),
        ("TRAILING", "value"),
    ];

    // Act
    let parsed = environment_file_contents::parse(PROBE);

    // Assert
    assert_eq!(
        variables(PROBE),
        expected
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value.to_owned()))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        parsed.ignored_lines, 1,
        "`export EXPORTED=maybe`, which systemd also set nothing from"
    );
}
