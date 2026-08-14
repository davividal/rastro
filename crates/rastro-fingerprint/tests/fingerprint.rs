use rastro_fingerprint::FingerprintError;
use rastro_fingerprint::Observation;
use rastro_fingerprint::{CollectorCategory, CollectorId, CollectorIdentity, CollectorVersion};
use rastro_fingerprint::{Facet, FacetName, FacetOutcome, Fingerprint};

fn facet_named(name: &str, category: CollectorCategory) -> Facet {
    Facet::new(
        FacetName::new(name).expect("test facet names should be legal"),
        CollectorIdentity::new(
            CollectorId::new(name).expect("test collector ids should be legal"),
            CollectorVersion::new("1").expect("test collector versions should be legal"),
        ),
        category,
        FacetOutcome::ok(Observation::null()),
    )
}

#[test]
fn facet_name_rejects_an_empty_value() {
    // Act & Assert
    assert_eq!(
        FacetName::new(""),
        Err(FingerprintError::EmptyIdentifier { kind: "facet name" })
    );
}

#[test]
fn facet_name_rejects_uppercase_letters() {
    // Act & Assert
    assert_eq!(
        FacetName::new("Nginx"),
        Err(FingerprintError::MalformedIdentifier {
            kind: "facet name",
            value: "Nginx".to_owned()
        })
    );
}

#[test]
fn facet_name_rejects_a_path_traversal_attempt() {
    // Act & Assert
    assert!(FacetName::new("../etc/passwd").is_err());
}

#[test]
fn facet_name_accepts_lowercase_digits_hyphen_and_underscore() {
    // Act
    let name = FacetName::new("systemd_units-2").expect("this should be a legal facet name");

    // Assert
    assert_eq!(name.as_str(), "systemd_units-2");
}

#[test]
fn from_facets_orders_facets_by_name() {
    // Act
    let fingerprint = Fingerprint::from_facets([
        facet_named("processes", CollectorCategory::State),
        facet_named("fs", CollectorCategory::State),
        facet_named("mounts", CollectorCategory::State),
    ])
    .expect("distinct facet names should be accepted");

    // Assert
    let names: Vec<&str> = fingerprint
        .facets()
        .iter()
        .map(|facet| facet.name.as_str())
        .collect();
    assert_eq!(names, ["fs", "mounts", "processes"]);
}

#[test]
fn from_facets_rejects_two_facets_sharing_a_name() {
    // Act
    let result = Fingerprint::from_facets([
        facet_named("nginx", CollectorCategory::State),
        facet_named("nginx", CollectorCategory::State),
    ]);

    // Assert
    assert_eq!(
        result,
        Err(FingerprintError::DuplicateFacetName {
            name: "nginx".to_owned()
        })
    );
}

#[test]
fn facets_in_selects_only_the_requested_category() {
    // Arrange
    let fingerprint = Fingerprint::from_facets([
        facet_named("host", CollectorCategory::Metadata),
        facet_named("mounts", CollectorCategory::State),
        facet_named("invocation", CollectorCategory::Metadata),
    ])
    .expect("distinct facet names should be accepted");

    // Act
    let metadata: Vec<&str> = fingerprint
        .facets_in(CollectorCategory::Metadata)
        .map(|facet| facet.name.as_str())
        .collect();

    // Assert
    assert_eq!(metadata, ["host", "invocation"]);
}
