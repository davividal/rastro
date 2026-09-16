//! Reading `/etc/environment` the way `pam_env` reads it.
//!
//! **Every expectation here was measured, not taken from `pam_env(8)`.** A probe file was
//! written, a PAM login performed under `libpam-modules` 1.7.0 on Debian 13, and the
//! resulting environment read back.
//!
//! # This is not the systemd format, and three rules are its exact opposite
//!
//! The file looks like a unit's `EnvironmentFile=` and is parsed by a different program with
//! different rules. Sharing a parser between them would be wrong in both directions:
//!
//! | line | `pam_env` | systemd |
//! |---|---|---|
//! | `export V=x` | sets `V` | drops the line |
//! | `V=x␠␠␠` | keeps the spaces | trims them |
//! | `V=a # b` | truncates at the `#` | keeps `# b` in the value |

use rastro::collectors::pam::environment_file;

fn variables(text: &str) -> Vec<(String, String)> {
    environment_file::parse(text)
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
fn a_plain_assignment_is_read() {
    assert_eq!(value_of("SIMPLE=plain\n"), "plain");
}

#[test]
fn an_export_prefix_is_stripped_rather_than_refusing_the_line() {
    // Arrange & Assert: the opposite of systemd, which sets nothing from this line because
    // the name would hold a space. `pam_env` takes the prefix off and sets the variable, so
    // a shared parser would report a variable the other reader does not set — or miss one
    // this reader does.
    assert_eq!(value_of("export EXPORTED=maybe\n"), "maybe");
}

#[test]
fn an_export_prefix_needs_a_space_and_not_a_tab() {
    // Arrange: measured. A tab leaves the name as `export\tV`, which is not an identifier.
    let parsed = environment_file::parse("export\tV=tabbed\n");

    // Assert
    assert!(parsed.variables.is_empty());
    assert_eq!(parsed.ignored_lines, 1);
}

#[test]
fn a_hash_truncates_the_line_wherever_it_appears() {
    // Arrange & Assert: measured, and the opposite of the systemd reader, where `#` past the
    // start of a line is an ordinary character. Quoting does not protect it either: the
    // truncation happens before the quotes are looked at.
    assert_eq!(value_of("V=a#b\n"), "a");
    assert_eq!(value_of("V=val # not a comment?\n"), "val ");
    assert_eq!(value_of("V=\"a # b\"\n"), "a ");
    assert_eq!(value_of("V=#only\n"), "");
}

#[test]
fn a_comment_line_and_a_blank_line_set_nothing() {
    let parsed = environment_file::parse("# a comment\n\n   # indented\nV=x\n");

    assert_eq!(
        variables("# a comment\n\n   # indented\nV=x\n"),
        vec![("V".to_owned(), "x".to_owned())]
    );
    assert_eq!(
        parsed.ignored_lines, 0,
        "a comment is not a line that failed to parse"
    );
}

#[test]
fn whitespace_is_kept_where_systemd_would_trim_it() {
    // Arrange & Assert: measured, both sides of the value.
    assert_eq!(value_of("V=value   \n"), "value   ");
    assert_eq!(value_of("V=  spaced\n"), "  spaced");
    assert_eq!(value_of("   V=indented\n"), "indented");
}

#[test]
fn whitespace_around_the_equals_refuses_the_line() {
    // Arrange: measured — `V = x` sets nothing, because the name is `V ` and that is not an
    // identifier. systemd trims and sets it, which is the third place the two disagree.
    let parsed = environment_file::parse("V = x\n");

    // Assert
    assert!(parsed.variables.is_empty());
    assert_eq!(parsed.ignored_lines, 1);
}

#[test]
fn one_leading_quote_is_stripped_and_a_trailing_one_only_when_it_ends_the_value() {
    // Arrange & Assert: measured, and the asymmetry is real rather than a simplification.
    // `"ab"␠␠` keeps its closing quote because a space follows it.
    assert_eq!(value_of("V=\"two words\"\n"), "two words");
    assert_eq!(value_of("V='two words'\n"), "two words");
    assert_eq!(value_of("V=\"ab\"  \n"), "ab\"  ");
}

#[test]
fn no_escape_is_processed_inside_a_value() {
    // Arrange & Assert: measured. `pam_env` hands the bytes through, so a backslash is a
    // backslash — unlike the systemd reader, where `\"` is a quote.
    assert_eq!(value_of("V=\"has\\\"quote\"\n"), "has\\\"quote");
}

#[test]
fn a_value_is_split_on_the_first_equals_only() {
    assert_eq!(value_of("EQUALS=a=b=c\n"), "a=b=c");
}

#[test]
fn an_empty_value_is_a_variable_that_is_set() {
    assert_eq!(value_of("EMPTY=\n"), "");
}

#[test]
fn the_last_assignment_of_a_repeated_name_wins() {
    assert_eq!(value_of("V=first\nV=second\n"), "second");
}

#[test]
fn a_name_that_is_not_an_identifier_is_counted_rather_than_reported() {
    // Arrange: measured — neither reaches the session.
    let parsed = environment_file::parse("BADNAME-X=x\n1BAD=y\nOK_NAME=z\n");

    // Assert
    assert_eq!(parsed.variables.len(), 1);
    assert_eq!(parsed.ignored_lines, 2);
}

#[test]
fn a_last_line_without_a_newline_still_counts() {
    assert_eq!(value_of("V=notrail"), "notrail");
}
