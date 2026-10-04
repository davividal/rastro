//! Which credential a run sends Elasticsearch, taken from the operator's credentials.
//!
//! One for every node on the box, in v1: telling two clusters' credentials apart needs the
//! cluster's name, which is behind the credential. See `docs/decisions.md`.

use rastro::collectors::elasticsearch::ApiCredential;
use rastro::credentials::Credentials;

fn credential(text: &str) -> Result<Option<ApiCredential>, String> {
    ApiCredential::from_credentials(&Credentials::parse(text).expect("credentials"))
}

#[test]
fn an_api_key_is_sent_as_an_api_key() {
    // Act
    let credential = credential("ELASTICSEARCH_API_KEY=a2V5OnNlY3JldA==\n")
        .expect("a credential")
        .expect("one was given");

    // Assert
    assert_eq!(credential.authorization(), "ApiKey a2V5OnNlY3JldA==");
}

#[test]
fn a_username_and_password_are_sent_as_basic_authentication() {
    // Act
    let credential =
        credential("ELASTICSEARCH_USERNAME=elastic\nELASTICSEARCH_PASSWORD=changeme\n")
            .expect("a credential")
            .expect("one was given");

    // Assert: `elastic:changeme` in base64.
    assert_eq!(credential.authorization(), "Basic ZWxhc3RpYzpjaGFuZ2VtZQ==");
}

#[test]
fn no_elasticsearch_credential_is_none() {
    // Act & Assert
    assert_eq!(credential("RABBITMQ_PASSWORD=x\n"), Ok(None));
}

#[test]
fn a_username_without_a_password_is_refused() {
    // Act
    let refusal = credential("ELASTICSEARCH_USERNAME=elastic\n").expect_err("half a credential");

    // Assert
    assert!(refusal.contains("ELASTICSEARCH_PASSWORD"), "{refusal}");
}

#[test]
fn an_api_key_beside_a_password_is_refused_rather_than_one_picked() {
    // Act
    let refusal =
        credential("ELASTICSEARCH_API_KEY=k\nELASTICSEARCH_USERNAME=u\nELASTICSEARCH_PASSWORD=p\n")
            .expect_err("two credentials");

    // Assert
    assert!(refusal.contains("ELASTICSEARCH_API_KEY"), "{refusal}");
}

#[test]
fn a_credential_never_shows_its_secret_when_formatted() {
    // Act
    let credential = credential("ELASTICSEARCH_USERNAME=u\nELASTICSEARCH_PASSWORD=s3cret\n")
        .expect("a credential")
        .expect("one was given");

    // Assert
    assert!(!format!("{credential:?}").contains("s3cret"));
}
