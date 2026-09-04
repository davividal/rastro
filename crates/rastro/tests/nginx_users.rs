//! The basic-auth user file, and which verifiers may be digested at all.

use rastro::collectors::nginx::htpasswd;
use rastro::collectors::nginx::value_objects::PasswordScheme;

#[test]
fn every_scheme_says_whether_its_verifier_carries_a_salt() {
    // Arrange: the rule that decides whether a digest reaches the document. A salted
    // verifier differs on every box and every rotation; an unsalted one is a pure function
    // of the password, so a digest of it would be an offline oracle over the document.
    // Act & Assert
    for (verifier, scheme, salted) in [
        ("$apr1$abc$def", PasswordScheme::Apr1, true),
        ("$2y$05$abc", PasswordScheme::Bcrypt, true),
        ("$2a$05$abc", PasswordScheme::Bcrypt, true),
        ("$2b$05$abc", PasswordScheme::Bcrypt, true),
        ("$5$abc$def", PasswordScheme::ShaCrypt, true),
        ("$6$abc$def", PasswordScheme::ShaCrypt, true),
        (
            "{SHA}qUqP5cyxm6YcTAhz05Hph5gvu9M=",
            PasswordScheme::Sha1,
            false,
        ),
        ("hunter2", PasswordScheme::Unrecognised, false),
    ] {
        let read = PasswordScheme::of(verifier);
        assert_eq!(read, scheme, "{verifier}");
        assert_eq!(read.is_salted(), salted, "{verifier}");
        assert!(!read.as_str().is_empty());
    }
}

#[test]
fn comments_and_blank_lines_are_not_users() {
    // Act
    let users = htpasswd::parse("# the wall\n\nalice:$apr1$abc$def\n").expect("a user file");

    // Assert
    assert_eq!(users.len(), 1);
    assert_eq!(users[0].name.as_str(), "alice");
}

#[test]
fn a_line_that_names_no_user_is_a_failure() {
    // Arrange: without a colon there is no user and no verifier, so the file is not the
    // format nginx reads. Passing over the line would report a wall with fewer people
    // behind it than it has.
    // Act
    let refused = htpasswd::parse("alice:$apr1$abc$def\nnonsense\n")
        .expect_err("the second line names no user");

    // Assert
    assert!(refused.to_string().contains("line 2"), "{refused}");
}
