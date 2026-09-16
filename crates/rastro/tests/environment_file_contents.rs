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

use rastro::collectors::systemd::EnvironmentFile;
use rastro::collectors::units::environment_file_contents;
use rastro::collectors::units::{EnvironmentReading, EnvironmentSource};

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

/// The single source a literal declaration yields.
///
/// A declaration expands to a list now, because a wildcard names several files. Every test
/// below names a literal path, so exactly one entry is the expected shape and a second would
/// be a defect this helper should surface rather than hide.
fn one_source(declared: EnvironmentFile) -> EnvironmentSource {
    let mut sources = environment_file_contents::read(declared);
    assert_eq!(sources.len(), 1, "a literal path names one file");

    sources.remove(0)
}

/// A scratch directory this test binary owns, under the target tree rather than the box.
fn scratch(name: &str) -> std::path::PathBuf {
    let directory = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::create_dir_all(&directory).expect("a scratch directory");

    directory
}

#[test]
fn reading_a_real_file_reports_what_it_sets() {
    // Arrange
    let path = scratch("read").join("app.env");
    std::fs::write(&path, "DATABASE_URL=postgres://x\nexport NOPE=1\n")
        .expect("the scratch file should be writable");
    let declared = EnvironmentFile::new(path.to_str().expect("a UTF-8 path"), false)
        .expect("an absolute path");

    // Act
    let source = one_source(declared);

    // Assert
    match source.reading {
        EnvironmentReading::Read {
            variables,
            ignored_lines,
        } => {
            assert_eq!(variables.len(), 1);
            assert_eq!(
                ignored_lines, 1,
                "the `export` line, which systemd also drops"
            );
        }
        other => panic!("expected the file to be read, got {other:?}"),
    }
}

#[test]
fn a_missing_file_is_absent_rather_than_an_error() {
    // Arrange: routine for a file the unit marked `ignore_errors`, and the whole finding for
    // one it did not.
    let path = scratch("missing").join("never-written.env");
    let declared =
        EnvironmentFile::new(path.to_str().expect("a UTF-8 path"), true).expect("an absolute path");

    // Act
    let source = one_source(declared);

    // Assert
    assert_eq!(source.reading, EnvironmentReading::Absent);
}

#[test]
fn a_directory_where_a_file_was_declared_is_an_error_and_not_an_absence() {
    // Arrange: a directory stands in for the unreadable case, because it fails for every
    // caller. A mode-based fixture would not: `chmod 000` still reads as root, and this
    // suite is run both ways on purpose.
    let directory = scratch("not-a-file");
    let declared = EnvironmentFile::new(directory.to_str().expect("a UTF-8 path"), false)
        .expect("an absolute path");

    // Act
    let source = one_source(declared);

    // Assert: rastro was able to look and what it found would not read, which is a different
    // statement about the box from "there is nothing here".
    match source.reading {
        EnvironmentReading::Unreadable(why) => assert!(!why.is_empty(), "the reason is the point"),
        other => panic!("expected an unreadable file, got {other:?}"),
    }
}

#[test]
fn a_file_whose_last_line_ends_in_a_backslash_continues_into_nothing() {
    // Arrange: the continuation has no next line to join, so what it accumulated is the
    // last logical line rather than being dropped on the floor.
    let text = "A=one\nB=two \\";

    // Act & Assert: both survive, and the dangling continuation does not swallow `B`.
    assert_eq!(
        variables(text),
        vec![
            ("A".to_owned(), "one".to_owned()),
            ("B".to_owned(), "two".to_owned())
        ]
    );
}

#[test]
fn a_quoted_value_keeps_the_whitespace_it_was_given() {
    // Act & Assert: the trailing-whitespace trim applies to a bare value only. Quoting is
    // how a file says the spaces are part of the value, and `systemd` honours that.
    assert_eq!(value_of("PADDED=\"  spaced  \"\n"), "  spaced  ");
}

#[test]
fn a_trailing_backslash_is_a_continuation_even_at_the_end_of_the_file() {
    // Arrange & Assert: measured. A backslash at the end of the last line continues into
    // nothing rather than surviving as the value's last character, quoted or not, because
    // the continuation is resolved before the quoting is.
    assert_eq!(value_of("TRAILING=\"ends\\"), "ends");
}

#[test]
fn a_continuation_inside_quotes_joins_with_no_separator_of_its_own() {
    // Arrange & Assert: measured. `"a\` / `b"` is `ab`, not `a b` — the join adds nothing,
    // so the only separator is whatever the text already had before the backslash.
    assert_eq!(value_of("V=\"a\\\nb\"\n"), "ab");
}

#[test]
fn an_escaped_backslash_at_end_of_line_is_a_value_and_not_a_continuation() {
    // Arrange: measured. `"ends\\"` is a value ending in one backslash, and the assignment
    // on the next line survives. Counting the run of backslashes is the whole of it: an odd
    // number continues the line, an even number is literal and the line ends.
    //
    // Getting this wrong does not merely mis-read a value, it *loses an assignment* — the
    // next line is swallowed into this one — which is why it matters more than the escapes.
    // Act & Assert: an even run ends the line, an odd run continues it, and the difference
    // decides whether `W` exists at all.
    assert_eq!(
        variables("V=a\\\\\nW=b\n"),
        vec![
            ("V".to_owned(), "a\\".to_owned()),
            ("W".to_owned(), "b".to_owned())
        ],
        "an even run is a literal backslash and the line ends"
    );
    assert_eq!(
        variables("V=a\\\nW=b\n"),
        vec![("V".to_owned(), "aW=b".to_owned())],
        "an odd run escapes the newline, so the next line joins this one"
    );
    assert_eq!(
        variables("V=\"a\\\\\nW=b\n"),
        vec![
            ("V".to_owned(), "a\\".to_owned()),
            ("W".to_owned(), "b".to_owned())
        ],
        "the run is counted the same inside an unterminated quote"
    );
}

#[test]
fn a_quoted_value_does_not_span_physical_lines_without_a_continuation() {
    // Arrange: measured, and it is the half of the review comment that was wrong. systemd
    // does *not* carry a single-quoted value onto the next physical line: it truncates at
    // the newline exactly as this parser does, and the orphaned closing line sets nothing.
    let text = "MULTI='first\nsecond'\nAFTER=intact\n";

    // Act
    let parsed = environment_file_contents::parse(text);

    // Assert
    assert_eq!(
        variables(text),
        vec![
            ("AFTER".to_owned(), "intact".to_owned()),
            ("MULTI".to_owned(), "first".to_owned()),
        ]
    );
    assert_eq!(parsed.ignored_lines, 1, "the orphaned `second'` line");
}

#[test]
fn the_shells_expansion_characters_are_escapable_inside_double_quotes() {
    // Arrange & Assert: measured. systemd strips the backslash before `$` and a backtick as
    // well as before a quote and a backslash, so recording the backslash would put a
    // character in the document that is not in the process — and a wrong redaction digest
    // with it, since the digest is taken over whatever is recorded.
    assert_eq!(value_of("PRICE=\"cost\\$5\"\n"), "cost$5");
    assert_eq!(value_of("TICK=\"a\\`b\"\n"), "a`b");
}

#[test]
fn a_name_systemd_would_reject_sets_nothing_and_is_counted() {
    // Arrange: measured. Both of these reach the process as nothing at all, so a facet that
    // reported them would claim the service has variables it does not.
    let text = "BADNAME-X=x\n1BAD=y\nOK_NAME=z\n";

    // Act
    let parsed = environment_file_contents::parse(text);

    // Assert
    assert_eq!(
        variables(text),
        vec![("OK_NAME".to_owned(), "z".to_owned())]
    );
    assert_eq!(parsed.ignored_lines, 2, "punctuation, and a leading digit");
}

#[test]
fn a_leading_underscore_is_a_legal_name() {
    // Act & Assert: the grammar is a C identifier, so `_` opens one.
    assert_eq!(value_of("_PRIVATE=ok\n"), "ok");
}

#[test]
fn a_fifo_named_as_an_environment_file_is_refused_rather_than_opened() {
    // Arrange: a unit may name anything, and opening a FIFO blocks until a writer appears.
    // One such declaration would otherwise stop the whole fingerprint, so the type is
    // checked before anything is opened. **If this test ever hangs, that check is gone.**
    let path = scratch("fifo").join("pipe.env");
    let _ = std::fs::remove_file(&path);
    let made = std::process::Command::new("mkfifo")
        .arg(&path)
        .status()
        .expect("mkfifo should be runnable");
    assert!(made.success(), "the fixture needs a FIFO");
    let declared = EnvironmentFile::new(path.to_str().expect("a UTF-8 path"), false)
        .expect("an absolute path");

    // Act
    let source = one_source(declared);

    // Assert: an error and not an absence — rastro looked and found something it will not
    // read, which is a different statement about the box from "there is nothing here".
    match source.reading {
        EnvironmentReading::Unreadable(why) => assert!(
            why.contains("not a regular file"),
            "the reason should name the type, got: {why}"
        ),
        other => panic!("expected a refusal, got {other:?}"),
    }
    let _ = std::fs::remove_file(&path);
}

#[test]
fn an_environment_file_past_the_bound_is_refused_rather_than_read() {
    // Arrange: an environment file is small by construction, so one this size is already a
    // misconfiguration. The bound is what stops it becoming a failed run.
    let path = scratch("oversize").join("huge.env");
    let mut line = String::from("A=");
    line.push_str(&"x".repeat(2 * 1024 * 1024));
    std::fs::write(&path, line).expect("the scratch file should be writable");
    let declared = EnvironmentFile::new(path.to_str().expect("a UTF-8 path"), false)
        .expect("an absolute path");

    // Act
    let source = one_source(declared);

    // Assert
    match source.reading {
        EnvironmentReading::Unreadable(why) => {
            assert!(why.contains("past the"), "got: {why}");
        }
        other => panic!("expected a refusal, got {other:?}"),
    }
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_wildcard_declaration_is_expanded_and_every_match_is_read() {
    // Arrange: measured against systemd 257 — `systemctl show` reports the pattern with the
    // `*` intact, so reading the declaration directly finds nothing and reports the whole
    // set absent. That silently loses exactly the variables this facet exists to name.
    let directory = scratch("glob");
    std::fs::write(directory.join("a.env"), "FROM_A=alpha\nSHARED=from_a\n").expect("writable");
    std::fs::write(directory.join("b.env"), "FROM_B=beta\nSHARED=from_b\n").expect("writable");
    std::fs::write(directory.join("skip.txt"), "NOT_MINE=x\n").expect("writable");
    let pattern = directory.join("*.env");
    let declared = EnvironmentFile::new(pattern.to_str().expect("a UTF-8 path"), false)
        .expect("an absolute path");

    // Act
    let sources = environment_file_contents::read(declared);

    // Assert: one entry per matched file, in byte order, which is the order systemd applies
    // them in — a variable set in two matched files takes the later file's value. Every
    // entry still names the pattern it came from, because the pattern changes when the unit
    // is edited and the matched set changes when a file appears in the directory.
    let paths: Vec<String> = sources
        .iter()
        .map(|source| {
            source
                .resolved
                .as_ref()
                .expect("a matched file has a path")
                .as_str()
                .to_owned()
        })
        .collect();
    assert_eq!(paths.len(), 2, "`skip.txt` does not match, got {paths:?}");
    assert!(paths[0].ends_with("a.env"), "got {paths:?}");
    assert!(paths[1].ends_with("b.env"), "got {paths:?}");
    for source in &sources {
        assert!(
            source.declared.path.as_str().ends_with("*.env"),
            "every entry names the declaration it came from"
        );
    }
}

#[test]
fn a_wildcard_that_matches_nothing_is_still_one_entry() {
    // Arrange: a *required* wildcard matching nothing stops the unit from starting —
    // measured as `Result=resources` — so this is a finding rather than an empty set to
    // omit. Absence is state, and the declaration has to stay visible to carry it.
    let pattern = scratch("glob-empty").join("*.env");
    let declared = EnvironmentFile::new(pattern.to_str().expect("a UTF-8 path"), false)
        .expect("an absolute path");

    // Act
    let sources = environment_file_contents::read(declared);

    // Assert
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].reading, EnvironmentReading::Absent);
    assert!(
        sources[0].resolved.is_none(),
        "there is no file to name, and naming the pattern here would invent one"
    );
}
