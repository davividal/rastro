//! What `ACL LIST` becomes: who may connect, and what each account may do.

use rastro::collectors::redis::{AclList, Reply};
use rastro_collector::Observation;
use rastro_fingerprint::Sensitivity;

mod support;

use support::observation::{field, items_of, keys_of, text};

/// The verifier of `s3cr3t`: plain `sha256`, measured on three builds.
const VERIFIER: &str = "#4e738ca5563c06cfd0018299933d58db1dd8bf97f6973dc99bf6cdc64b5550bd";

fn users(lines: &[&str]) -> Observation {
    let reply = Reply::Array(
        lines
            .iter()
            .map(|line| Reply::Bulk((*line).to_owned()))
            .collect(),
    );

    Observation::from(&AclList::parse(reply).expect("a real reply"))
}

fn rules_of(users: &Observation, user: &str) -> Vec<Observation> {
    items_of(&field(users, user))
}

#[test]
fn each_account_keeps_its_rules_in_the_order_the_server_prints_them() {
    // Arrange: a rule can undo an earlier one, so their order is part of what they mean.
    let lines = [
        "user worker on #0123 ~jobs:* resetchannels -@all +get +set",
        "user default on nopass sanitize-payload ~* &* +@all",
    ];

    // Act
    let users = users(&lines);

    // Assert
    assert_eq!(keys_of(&users), ["default", "worker"]);
    let rules: Vec<String> = rules_of(&users, "default").iter().map(text).collect();
    assert_eq!(
        rules,
        ["on", "nopass", "sanitize-payload", "~*", "&*", "+@all"]
    );
}

#[test]
fn a_password_verifier_is_sensitive_and_nothing_else_is() {
    // Arrange: unsalted, so its stand-in is guessable; carried because a rotation must show,
    // and `requirepass` is the same secret already carried the same way.
    let line = format!("user default on {VERIFIER} ~* &* +@all");

    // Act
    let users = users(&[&line]);

    // Assert
    let rules = rules_of(&users, "default");
    assert_eq!(rules[1].sensitivity(), Sensitivity::Sensitive);
    assert_eq!(rules[0].sensitivity(), Sensitivity::Public);
    assert_eq!(rules[2].sensitivity(), Sensitivity::Public);
}

#[test]
fn a_line_that_is_not_an_account_is_refused() {
    // Act
    let result = AclList::parse(Reply::Array(vec![Reply::Bulk("group admins".to_owned())]));

    // Assert
    assert!(result.is_err(), "{result:?}");
}

#[test]
fn an_account_named_twice_is_refused() {
    // Act: the server names each account once, so a second one means the reply was misread.
    let result = AclList::parse(Reply::Array(vec![
        Reply::Bulk("user default on nopass +@all".to_owned()),
        Reply::Bulk("user default off".to_owned()),
    ]));

    // Assert
    assert!(result.is_err(), "{result:?}");
}
