//! Reading one directive out of a redis configuration file the way the server reads it.
//!
//! Each case here is one where reading differently from the server means sending it a wrong
//! password, which is an entry in its `ACL LOG`.

use std::path::PathBuf;

use rastro::collectors::redis::requirepass_in;

mod support;

use support::fs_tree::{scratch_tree, write};

fn file_with(name: &str, contents: &str) -> PathBuf {
    let root = scratch_tree(name, &[]);
    write(&root, "redis.conf", contents);

    root.join("redis.conf")
}

fn password_in(name: &str, contents: &str) -> Option<String> {
    requirepass_in(&file_with(name, contents)).expect("a readable file")
}

#[test]
fn a_plain_password_is_read() {
    // Act & Assert
    assert_eq!(
        password_in(
            "redis-pass-plain",
            "# requirepass commented\nrequirepass hunter2\n"
        ),
        Some("hunter2".to_owned())
    );
}

#[test]
fn the_directive_is_matched_whatever_its_case() {
    // Act & Assert: redis lowercases the directive before matching it.
    assert_eq!(
        password_in("redis-pass-case", "RequirePass hunter2\n"),
        Some("hunter2".to_owned())
    );
}

#[test]
fn a_single_quoted_password_keeps_its_backslashes() {
    // Act & Assert: only `\'` is an escape inside single quotes.
    assert_eq!(
        password_in("redis-pass-single", "requirepass 'a\\nb\\'c'\n"),
        Some("a\\nb'c".to_owned())
    );
}

#[test]
fn a_hexadecimal_escape_is_the_byte_it_names() {
    // Act & Assert
    assert_eq!(
        password_in("redis-pass-hex", "requirepass \"\\x41\\x42\"\n"),
        Some("AB".to_owned())
    );
}

#[test]
fn an_empty_password_clears_an_earlier_one() {
    // Act & Assert: `requirepass ""` is how a password is switched off in the file.
    assert_eq!(
        password_in(
            "redis-pass-cleared",
            "requirepass hunter2\nrequirepass \"\"\n"
        ),
        None
    );
}

#[test]
fn a_file_without_the_directive_sets_no_password() {
    // Act & Assert
    assert_eq!(password_in("redis-pass-none", "bind 127.0.0.1\n"), None);
}

#[test]
fn unbalanced_quotes_are_refused() {
    // Act
    let result = requirepass_in(&file_with(
        "redis-pass-unbalanced",
        "requirepass \"hunter2\n",
    ));

    // Assert: the server would have refused to start, so nothing here is its password.
    assert!(result.is_err(), "{result:?}");
}

#[test]
fn a_byte_escape_outside_ascii_is_refused_rather_than_approximated() {
    // Act
    let result = requirepass_in(&file_with("redis-pass-byte", "requirepass \"\\xff\"\n"));

    // Assert
    assert!(result.is_err(), "{result:?}");
}

#[test]
fn a_relative_include_is_refused() {
    // Act
    let result = requirepass_in(&file_with("redis-pass-relative", "include local.conf\n"));

    // Assert: resolved against a working directory nothing records.
    let error = result.expect_err("a relative include").to_string();
    assert!(error.contains("relative"), "{error}");
}

#[test]
fn a_file_that_includes_itself_is_refused() {
    // Arrange
    let file = file_with("redis-pass-cycle", "");
    write(
        file.parent().expect("a parent"),
        "redis.conf",
        &format!("include {}\n", file.display()),
    );

    // Act
    let result = requirepass_in(&file);

    // Assert
    let error = result.expect_err("a cycle").to_string();
    assert!(error.contains("including itself"), "{error}");
}

#[test]
fn a_pattern_include_is_read_in_byte_order() {
    // Arrange: two drop-ins, the later of which the server reads last.
    let root = scratch_tree("redis-pass-glob", &["conf.d"]);
    write(&root, "conf.d/10-base.conf", "requirepass first\n");
    write(&root, "conf.d/20-local.conf", "requirepass second\n");
    write(
        &root,
        "redis.conf",
        &format!("include {}/conf.d/*.conf\n", root.display()),
    );

    // Act & Assert
    assert_eq!(
        requirepass_in(&root.join("redis.conf")).expect("a readable file"),
        Some("second".to_owned())
    );
}
