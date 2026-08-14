use rastro_fingerprint::FingerprintError;
use rastro_fingerprint::{CollectorId, CollectorVersion};

#[test]
fn collector_id_rejects_an_empty_value() {
    // Act & Assert
    assert_eq!(
        CollectorId::new(""),
        Err(FingerprintError::EmptyIdentifier {
            kind: "collector id"
        })
    );
}

#[test]
fn collector_id_rejects_uppercase_letters() {
    // Act & Assert
    assert_eq!(
        CollectorId::new("Nginx"),
        Err(FingerprintError::MalformedIdentifier {
            kind: "collector id",
            value: "Nginx".to_owned()
        })
    );
}

#[test]
fn collector_id_accepts_lowercase_digits_hyphen_and_underscore() {
    // Act
    let id = CollectorId::new("systemd_units-2").expect("this should be a legal collector id");

    // Assert
    assert_eq!(id.as_str(), "systemd_units-2");
}

#[test]
fn collector_version_rejects_an_empty_value() {
    // Act & Assert
    assert_eq!(
        CollectorVersion::new(""),
        Err(FingerprintError::EmptyIdentifier {
            kind: "collector version"
        })
    );
}

#[test]
fn collector_version_rejects_a_value_containing_whitespace() {
    // Act & Assert
    assert_eq!(
        CollectorVersion::new("1.0 beta"),
        Err(FingerprintError::WhitespaceInIdentifier {
            kind: "collector version",
            value: "1.0 beta".to_owned()
        })
    );
}

#[test]
fn collector_version_accepts_the_shapes_collector_authors_actually_use() {
    // Act & Assert
    assert!(CollectorVersion::new("1").is_ok());
    assert!(CollectorVersion::new("0.3.2-rc1").is_ok());
    assert!(CollectorVersion::new("a1b2c3d").is_ok());
    assert!(CollectorVersion::new("2026-08-13").is_ok());
}
