//! Reading one directive out of a redis configuration file the way the server reads it.
//!
//! Each case here is one where reading differently from the server means sending it a wrong
//! password, which is an entry in its `ACL LOG`.

use std::path::{Path, PathBuf};

use rastro::collectors::redis::{default_user_in_acl_file, password_directives_in};

mod support;

/// A server in rastro's own root, whose paths mean the same on both sides.
const HOST_ROOT: &str = "/";

use support::fs_tree::{scratch_tree, write};

fn file_with(name: &str, contents: &str) -> PathBuf {
    let root = scratch_tree(name, &[]);
    write(&root, "redis.conf", contents);

    root.join("redis.conf")
}

fn password_in(name: &str, contents: &str) -> Option<String> {
    password_directives_in(Path::new(HOST_ROOT), &file_with(name, contents))
        .expect("a readable file")
        .requirepass
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
    let result = password_directives_in(
        Path::new(HOST_ROOT),
        &file_with("redis-pass-unbalanced", "requirepass \"hunter2\n"),
    );

    // Assert: the server would have refused to start, so nothing here is its password.
    assert!(result.is_err(), "{result:?}");
}

#[test]
fn a_byte_escape_outside_ascii_is_refused_rather_than_approximated() {
    // Act
    let result = password_directives_in(
        Path::new(HOST_ROOT),
        &file_with("redis-pass-byte", "requirepass \"\\xff\"\n"),
    );

    // Assert
    assert!(result.is_err(), "{result:?}");
}

#[test]
fn a_relative_include_is_refused() {
    // Act
    let result = password_directives_in(
        Path::new(HOST_ROOT),
        &file_with("redis-pass-relative", "include local.conf\n"),
    );

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
    let result = password_directives_in(Path::new(HOST_ROOT), &file);

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
        password_directives_in(Path::new(HOST_ROOT), &root.join("redis.conf"))
            .expect("a readable file")
            .requirepass,
        Some("second".to_owned())
    );
}

#[test]
fn the_default_accounts_line_is_read_with_its_rules_in_order() {
    // Arrange: what `CONFIG REWRITE` appends, beside another account that is not the default.
    let file = file_with(
        "redis-pass-user-line",
        "requirepass hunter2\nuser alice on >a ~* +@all\nUser default on \">two words\" ~* &* +@all\n",
    );

    // Act
    let directives = password_directives_in(Path::new(HOST_ROOT), &file).expect("a readable file");

    // Assert: quoted as the server splits it, and the directive matched whatever its case.
    assert_eq!(
        directives.default_user,
        Some(vec![
            "on".to_owned(),
            ">two words".to_owned(),
            "~*".to_owned(),
            "&*".to_owned(),
            "+@all".to_owned(),
        ])
    );
    assert_eq!(directives.requirepass, Some("hunter2".to_owned()));
}

#[test]
fn the_default_account_declared_twice_is_refused() {
    // Act: measured, the server refuses to start on such a file.
    let result = password_directives_in(
        Path::new(HOST_ROOT),
        &file_with(
            "redis-pass-user-twice",
            "user default on >one\nuser default on >two\n",
        ),
    );

    // Assert
    assert!(result.is_err(), "{result:?}");
}

#[test]
fn an_acl_file_is_named() {
    // Act
    let directives = password_directives_in(
        Path::new(HOST_ROOT),
        &file_with(
            "redis-pass-aclfile",
            "requirepass ignored\naclfile /etc/redis/users.acl\n",
        ),
    )
    .expect("a readable file");

    // Assert
    assert_eq!(
        directives.acl_file.as_deref(),
        Some(std::path::Path::new("/etc/redis/users.acl"))
    );
}

#[test]
fn the_default_account_is_read_out_of_an_acl_file() {
    // Arrange
    let root = scratch_tree("redis-pass-acl-default", &[]);
    write(
        &root,
        "users.acl",
        "# accounts\nuser alice on >a ~* +@all\nuser default on >fromfile ~* &* +@all\n",
    );

    // Act & Assert
    assert_eq!(
        default_user_in_acl_file(Path::new(HOST_ROOT), &root.join("users.acl"))
            .expect("a readable file"),
        Some(
            ["on", ">fromfile", "~*", "&*", "+@all"]
                .map(str::to_owned)
                .to_vec()
        )
    );
}

#[test]
fn an_acl_file_without_the_default_account_says_so() {
    // Arrange
    let root = scratch_tree("redis-pass-acl-no-default", &[]);
    write(&root, "users.acl", "user alice on >a ~* +@all\n");

    // Act & Assert: measured, the server then leaves the default account without a password.
    assert_eq!(
        default_user_in_acl_file(Path::new(HOST_ROOT), &root.join("users.acl"))
            .expect("a readable file"),
        None
    );
}

#[test]
fn an_acl_file_declaring_the_default_account_twice_is_refused() {
    // Arrange
    let root = scratch_tree("redis-pass-acl-twice", &[]);
    write(
        &root,
        "users.acl",
        "user default on >one ~* +@all\nuser default on >two ~* +@all\n",
    );

    // Act
    let result = default_user_in_acl_file(Path::new(HOST_ROOT), &root.join("users.acl"));

    // Assert
    assert!(result.is_err(), "{result:?}");
}

#[test]
fn a_fifo_where_the_file_should_be_is_refused_rather_than_waited_on() {
    // Arrange: the file is the redis account's, so a FIFO is its to put there; opened for
    // reading as it stands, it blocks until a writer appears, which is never.
    let root = scratch_tree("redis-pass-fifo", &[]);
    let fifo = root.join("redis.conf");
    let made = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .expect("mkfifo runs");
    assert!(made.success());

    // Act
    let result = password_directives_in(Path::new(HOST_ROOT), &fifo);

    // Assert
    let error = result.expect_err("a FIFO").to_string();
    assert!(error.contains("not a regular file"), "{error}");
}

#[test]
fn a_device_where_an_include_should_be_is_refused() {
    // Act: `/dev/zero` never ends, and read whole it grows without end.
    let result = password_directives_in(
        Path::new(HOST_ROOT),
        &file_with(
            "redis-pass-device",
            "include /dev/zero\nrequirepass hunter2\n",
        ),
    );

    // Assert
    let error = result.expect_err("a device").to_string();
    assert!(error.contains("not a regular file"), "{error}");
}

#[test]
fn a_file_past_the_size_bound_is_refused() {
    // Act
    let result = password_directives_in(
        Path::new(HOST_ROOT),
        &file_with("redis-pass-huge", &"# padding\n".repeat(120_000)),
    );

    // Assert
    let error = result.expect_err("a huge file").to_string();
    assert!(error.contains("larger than"), "{error}");
}

#[test]
fn includes_that_fan_out_stop_at_a_total_budget() {
    // Arrange: measured, fifteen files each including the next three times took 35 s; depth
    // alone bounds nesting, not work.
    let root = scratch_tree("redis-pass-fanout", &[]);
    for level in 0..15 {
        let next = root.join(format!("level-{}.conf", level + 1));
        write(
            &root,
            &format!("level-{level}.conf"),
            &format!("include {0}\ninclude {0}\ninclude {0}\n", next.display()),
        );
    }
    write(&root, "level-15.conf", "requirepass hunter2\n");

    // Act
    let started = std::time::Instant::now();
    let result = password_directives_in(Path::new(HOST_ROOT), &root.join("level-0.conf"));

    // Assert: refused, and quickly.
    let error = result.expect_err("a fan-out").to_string();
    assert!(error.contains("files"), "{error}");
    assert!(started.elapsed() < std::time::Duration::from_secs(2));
}

#[test]
fn a_wildcard_include_over_a_huge_directory_is_refused_before_it_is_listed_in_full() {
    // Arrange: the directory is the redis account's to fill, and every entry is listed before the
    // file budget sees a match; none of these match, so only a bound on the listing stops it.
    let root = scratch_tree("redis-pass-wide-directory", &["owned"]);
    for entry in 0..=10_000 {
        write(&root.join("owned"), &format!("{entry}.txt"), "");
    }
    let file = file_with(
        "redis-pass-wide-include",
        &format!("include {}/*.conf\n", root.join("owned").display()),
    );

    // Act
    let result = password_directives_in(Path::new(HOST_ROOT), &file);

    // Assert
    let error = result.expect_err("a huge directory").to_string();
    assert!(error.contains("entries"), "{error}");
}

#[test]
fn an_included_name_that_is_not_utf8_is_refused_rather_than_read_as_another() {
    // Arrange: a drop-in whose name is not UTF-8 beside one spelled as its lossy reading, so a
    // reader that took names as text would open the second twice and never read the first, whose
    // password the server applied last.
    use std::os::unix::ffi::OsStrExt;

    let root = scratch_tree("redis-pass-not-utf8", &["conf.d"]);
    let directory = root.join("conf.d");
    let raw = std::ffi::OsStr::from_bytes(b"\xff.conf");
    std::fs::write(directory.join(raw), "requirepass hunter2\n").expect("a non-UTF-8 name");
    write(&directory, "\u{FFFD}.conf", "requirepass other\n");
    let file = file_with(
        "redis-pass-not-utf8-include",
        &format!("include {}/*.conf\n", directory.display()),
    );

    // Act
    let result = password_directives_in(Path::new(HOST_ROOT), &file);

    // Assert
    let error = result.expect_err("an unreadable name").to_string();
    assert!(error.contains("UTF-8"), "{error}");
}

#[test]
fn a_wildcard_outside_the_last_component_is_refused_rather_than_resolved() {
    // Arrange: every packaged layout globs a drop-in directory's files, never the directory.
    let file = file_with("redis-pass-directory-glob", "include /etc/*/redis.conf\n");

    // Act
    let result = password_directives_in(Path::new(HOST_ROOT), &file);

    // Assert
    let error = result.expect_err("an unresolved pattern").to_string();
    assert!(error.contains("does not resolve"), "{error}");
}

#[test]
fn a_relative_configuration_file_is_refused_rather_than_read_from_rastros_directory() {
    // Act: the server resolved it against the unit's working directory, which nothing records.
    let result = password_directives_in(Path::new(HOST_ROOT), Path::new("conf/redis.conf"));

    // Assert
    let error = result.expect_err("a relative file").to_string();
    assert!(error.contains("relative"), "{error}");
}

#[test]
fn a_backslash_x_without_two_hex_digits_is_the_letter_x() {
    // Act & Assert: measured, redis takes `"a\xzb"` as `axzb`.
    assert_eq!(
        password_in("redis-pass-x-literal", "requirepass \"a\\xzb\"\n"),
        Some("axzb".to_owned())
    );
}

#[test]
fn only_ascii_whitespace_separates_words() {
    // Act & Assert: redis splits on C `isspace`, so a non-breaking space is part of the word.
    assert_eq!(
        password_in("redis-pass-nbsp", "requirepass pass\u{a0}word\n"),
        Some("pass\u{a0}word".to_owned())
    );
}

#[test]
fn the_directives_never_print_their_password() {
    // Act
    let directives = password_directives_in(
        Path::new(HOST_ROOT),
        &file_with(
            "redis-pass-debug",
            "requirepass printed-nowhere\nuser default on >also-nowhere\n",
        ),
    )
    .expect("a readable file");

    // Assert: a `{:?}` in some later message must not carry the password.
    let printed = format!("{directives:?}");
    assert!(!printed.contains("printed-nowhere"), "{printed}");
    assert!(!printed.contains("also-nowhere"), "{printed}");
}
