//! Reading one directive out of a redis configuration file the way the server reads it.
//!
//! Each case here is one where reading differently from the server means sending it a wrong
//! password, which is an entry in its `ACL LOG`.

use std::path::PathBuf;

use rastro::collectors::redis::{default_user_in_acl_file, password_directives_in};

mod support;

use support::fs_tree::{scratch_tree, write};

fn file_with(name: &str, contents: &str) -> PathBuf {
    let root = scratch_tree(name, &[]);
    write(&root, "redis.conf", contents);

    root.join("redis.conf")
}

fn password_in(name: &str, contents: &str) -> Option<String> {
    password_directives_in(&file_with(name, contents))
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
    let result = password_directives_in(&file_with(
        "redis-pass-unbalanced",
        "requirepass \"hunter2\n",
    ));

    // Assert: the server would have refused to start, so nothing here is its password.
    assert!(result.is_err(), "{result:?}");
}

#[test]
fn a_byte_escape_outside_ascii_is_refused_rather_than_approximated() {
    // Act
    let result = password_directives_in(&file_with("redis-pass-byte", "requirepass \"\\xff\"\n"));

    // Assert
    assert!(result.is_err(), "{result:?}");
}

#[test]
fn a_relative_include_is_refused() {
    // Act
    let result = password_directives_in(&file_with("redis-pass-relative", "include local.conf\n"));

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
    let result = password_directives_in(&file);

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
        password_directives_in(&root.join("redis.conf"))
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
    let directives = password_directives_in(&file).expect("a readable file");

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
    let result = password_directives_in(&file_with(
        "redis-pass-user-twice",
        "user default on >one\nuser default on >two\n",
    ));

    // Assert
    assert!(result.is_err(), "{result:?}");
}

#[test]
fn an_acl_file_is_named() {
    // Act
    let directives = password_directives_in(&file_with(
        "redis-pass-aclfile",
        "requirepass ignored\naclfile /etc/redis/users.acl\n",
    ))
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
        default_user_in_acl_file(&root.join("users.acl")).expect("a readable file"),
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
        default_user_in_acl_file(&root.join("users.acl")).expect("a readable file"),
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
    let result = default_user_in_acl_file(&root.join("users.acl"));

    // Assert
    assert!(result.is_err(), "{result:?}");
}
