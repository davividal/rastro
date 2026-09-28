//! Which password the `default` account accepts, worked out from the files before anything is sent.
//!
//! Every rule here was measured on redis 8.0.2, because the cost of reading one wrong is not a
//! wrong document but a refused `AUTH`, which is an entry in the server's `ACL LOG`:
//!
//! - a `user default` line outranks `requirepass`, whatever their order;
//! - redis resets the account before applying the line, so the account is `off` without `on`;
//! - `CONFIG REWRITE` keeps `requirepass` and appends `user default on #<sha256>` beside it.

use rastro::collectors::redis::password_for_default_account;
use sha2::{Digest, Sha256};

fn hash_of(password: &str) -> String {
    Sha256::digest(password.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn rules(line: &str) -> Vec<String> {
    line.split_whitespace().map(str::to_owned).collect()
}

fn refusal(requirepass: Option<&str>, line: &str) -> String {
    password_for_default_account(requirepass, Some(&rules(line)))
        .expect_err("no password the account accepts")
}

#[test]
fn without_an_account_line_the_password_is_requirepass() {
    // Act & Assert
    assert_eq!(
        password_for_default_account(Some("hunter2"), None),
        Ok("hunter2".to_owned())
    );
}

#[test]
fn without_either_there_is_no_password_to_send() {
    // Act
    let result = password_for_default_account(None, None);

    // Assert
    let refusal = result.expect_err("nothing to send");
    assert!(refusal.contains("sets no password"), "{refusal}");
}

#[test]
fn an_account_line_outranks_requirepass() {
    // Act & Assert: measured, `AUTH` with the `requirepass` value answers `WRONGPASS`.
    assert_eq!(
        password_for_default_account(Some("stale"), Some(&rules("on >current ~* &* +@all"))),
        Ok("current".to_owned())
    );
}

#[test]
fn requirepass_is_sent_where_it_matches_the_hash_config_rewrite_wrote() {
    // Arrange: the file as `CONFIG REWRITE` leaves it.
    let line = format!("on #{} ~* &* +@all", hash_of("hunter2"));

    // Act & Assert: checked here, so the `AUTH` cannot fail on account of the file.
    assert_eq!(
        password_for_default_account(Some("hunter2"), Some(&rules(&line))),
        Ok("hunter2".to_owned())
    );
}

#[test]
fn a_requirepass_that_no_longer_matches_the_hash_is_never_sent() {
    // Arrange: the `requirepass` line edited after a rewrite, the generated line left alone.
    let line = format!("on #{} ~* &* +@all", hash_of("hunter2"));

    // Act
    let refusal = refusal(Some("edited-later"), &line);

    // Assert
    assert!(refusal.contains("hash"), "{refusal}");
}

#[test]
fn an_account_without_a_password_is_a_disagreement_with_a_server_that_asks_for_one() {
    // Act
    let refusal = refusal(Some("hunter2"), "on nopass ~* &* +@all");

    // Assert: measured, such a server answers without a password, so it is not this one.
    assert!(refusal.contains("without a password"), "{refusal}");
}

#[test]
fn an_account_never_switched_on_accepts_nothing() {
    // Act: redis resets the account first, and a reset account is `off`.
    let refusal = refusal(None, ">hunter2 ~* &* +@all");

    // Assert
    assert!(refusal.contains("switched off"), "{refusal}");
}

#[test]
fn a_password_removed_later_in_the_line_is_not_sent() {
    // Act & Assert
    assert_eq!(
        password_for_default_account(None, Some(&rules("on >first <first >second"))),
        Ok("second".to_owned())
    );
}

#[test]
fn resetpass_leaves_the_account_with_no_password_at_all() {
    // Act
    let refusal = refusal(Some("hunter2"), "on >hunter2 resetpass");

    // Assert
    assert!(refusal.contains("no password at all"), "{refusal}");
}

#[test]
fn a_password_after_nopass_takes_the_account_off_nopass() {
    // Act & Assert: `>` clears `nopass`, and `nopass` clears every password before it.
    assert_eq!(
        password_for_default_account(None, Some(&rules("on nopass >hunter2"))),
        Ok("hunter2".to_owned())
    );
    let refusal = refusal(None, "on >hunter2 nopass");
    assert!(refusal.contains("without a password"), "{refusal}");
}

#[test]
fn a_removed_hash_takes_its_password_with_it() {
    // Arrange
    let line = format!("on >hunter2 !{}", hash_of("hunter2"));

    // Act
    let refusal = refusal(None, &line);

    // Assert
    assert!(refusal.contains("no password at all"), "{refusal}");
}
