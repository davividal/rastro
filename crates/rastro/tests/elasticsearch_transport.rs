//! Whether a node may be asked over plain HTTP, decided from its settings before any request.
//!
//! v1 speaks plain HTTP only, and a plaintext request to a TLS listener is a request the node
//! did not want, so the answer has to be known without asking.

use rastro::collectors::elasticsearch::{NodeSettings, Transport};

const TLS_SETTING: &str = "xpack.security.http.ssl.enabled";

fn settings(pairs: &[(&str, &str)]) -> NodeSettings {
    pairs
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect()
}

#[test]
fn transport_is_plain_where_the_settings_say_nothing_of_tls() {
    // Arrange: 7.17's default, and an 8.x node with security switched off.
    let settings = settings(&[("xpack.security.enabled", "false")]);

    // Act & Assert
    assert_eq!(settings.transport(), Transport::Plain);
}

#[test]
fn transport_is_plain_where_tls_is_switched_off() {
    // Arrange
    let settings = settings(&[(TLS_SETTING, "false")]);

    // Act & Assert
    assert_eq!(settings.transport(), Transport::Plain);
}

#[test]
fn transport_requires_tls_where_the_settings_switch_it_on() {
    // Arrange: what 8.x's security auto-configuration writes into `elasticsearch.yml`.
    let settings = settings(&[(TLS_SETTING, "true")]);

    // Act & Assert
    assert_eq!(settings.transport(), Transport::Tls);
}

#[test]
fn transport_requires_tls_for_a_value_it_does_not_recognise() {
    // Arrange: not a boolean the node accepts. Plaintext is the wrong direction to be wrong in.
    let settings = settings(&[(TLS_SETTING, "yes")]);

    // Act & Assert
    assert_eq!(settings.transport(), Transport::Tls);
}
