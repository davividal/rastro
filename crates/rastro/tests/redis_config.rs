//! What `CONFIG GET *` becomes: the settings a server is running with.

use rastro::collectors::redis::{ConfigGet, Reply};
use rastro_collector::Observation;
use rastro_fingerprint::Sensitivity;

mod support;

use support::observation::{field, keys_of, text};

fn bulk(text: &str) -> Reply {
    Reply::Bulk(text.to_owned())
}

/// A reply in the order a server sent it, which is not an order anybody chose.
fn pairs(settings: &[(&str, &str)]) -> Reply {
    Reply::Array(
        settings
            .iter()
            .flat_map(|(name, value)| [bulk(name), bulk(value)])
            .collect(),
    )
}

#[test]
fn settings_are_keyed_by_name_in_a_fixed_order() {
    // Arrange: measured, two restarts of one binary on one file moved 849 of 872 lines of
    // `CONFIG GET *`, so wire order is noise.
    let reply = pairs(&[
        ("save", ""),
        ("maxmemory", "32212254720"),
        ("appendonly", "no"),
    ]);

    // Act
    let settings = Observation::from(&ConfigGet::parse(reply).expect("a real reply"));

    // Assert: and `save ""`, the estate's persistence switched off, stays an empty value.
    assert_eq!(keys_of(&settings), ["appendonly", "maxmemory", "save"]);
    assert_eq!(text(&field(&settings, "save")), "");
    assert_eq!(text(&field(&settings, "maxmemory")), "32212254720");
}

#[test]
fn every_credential_a_server_holds_in_its_settings_is_sensitive() {
    // Arrange: each arrives in plain text; `requirepass` is compared byte for byte at `AUTH`.
    let reply = pairs(&[
        ("requirepass", "hunter2"),
        ("masterauth", "hunter3"),
        // Measured: valkey 8.1 and later answer `CONFIG GET *` with both names, one value.
        ("primaryauth", "hunter3"),
        ("tls-key-file-pass", "hunter4"),
        ("tls-client-key-file-pass", "hunter5"),
        ("masteruser", "replicator"),
        ("maxmemory", "0"),
    ]);

    // Act
    let settings = Observation::from(&ConfigGet::parse(reply).expect("a real reply"));

    // Assert: the account a replica authenticates as names whose secret it is, and is not one.
    for name in [
        "requirepass",
        "masterauth",
        "primaryauth",
        "tls-key-file-pass",
        "tls-client-key-file-pass",
    ] {
        assert_eq!(
            field(&settings, name).sensitivity(),
            Sensitivity::Sensitive,
            "{name}"
        );
    }
    assert_eq!(
        field(&settings, "masteruser").sensitivity(),
        Sensitivity::Public
    );
    assert_eq!(
        field(&settings, "maxmemory").sensitivity(),
        Sensitivity::Public
    );
}

#[test]
fn a_reply_with_a_name_and_no_value_is_refused() {
    // Act
    let result = ConfigGet::parse(Reply::Array(vec![bulk("save")]));

    // Assert
    assert!(result.is_err(), "{result:?}");
}

#[test]
fn a_reply_that_is_not_text_pairs_is_refused() {
    // Act
    let result = ConfigGet::parse(Reply::Array(vec![bulk("port"), Reply::Integer(6379)]));

    // Assert
    assert!(result.is_err(), "{result:?}");
}

#[test]
fn an_unset_credential_is_shown_as_unset() {
    // Arrange: measured on a stock redis 8.0.2, `masterauth` is `""` on a server that is no
    // replica. A digest there would say a secret exists where none does.
    let reply = pairs(&[("masterauth", "")]);

    // Act
    let settings = Observation::from(&ConfigGet::parse(reply).expect("a real reply"));

    // Assert
    let masterauth = field(&settings, "masterauth");
    assert_eq!(masterauth.sensitivity(), Sensitivity::Public);
    assert_eq!(text(&masterauth), "");
}

#[test]
fn a_setting_named_twice_is_refused() {
    // Act: the server names each setting once, so a second is a misread reply.
    let result = ConfigGet::parse(pairs(&[("save", ""), ("save", "60 1")]));

    // Assert
    assert!(result.is_err(), "{result:?}");
}

#[test]
fn a_reply_of_the_wrong_shape_is_not_echoed_into_the_refusal() {
    // Arrange: a bulk string where a list belongs, as large as the redis account likes.
    let reply = Reply::Bulk("leaked ".repeat(10_000));

    // Act
    let error = ConfigGet::parse(reply)
        .expect_err("a wrong shape")
        .to_string();

    // Assert: the kind is named, the content is not.
    assert!(!error.contains("leaked"), "{error}");
    assert!(error.len() < 300, "{}", error.len());
}
