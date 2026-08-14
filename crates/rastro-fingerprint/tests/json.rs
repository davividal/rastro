use rastro_fingerprint::Observation;
use rastro_fingerprint::View;
use rastro_fingerprint::json::to_canonical_json;
use rastro_fingerprint::{CollectorCategory, CollectorId, CollectorIdentity, CollectorVersion};
use rastro_fingerprint::{Facet, FacetName, FacetOutcome, Fingerprint};
use serde_json::{Value, json};

fn facet(name: &str, category: CollectorCategory, outcome: FacetOutcome) -> Facet {
    Facet::new(
        FacetName::new(name).expect("test facet names should be legal"),
        CollectorIdentity::new(
            CollectorId::new(name).expect("test collector ids should be legal"),
            CollectorVersion::new("1").expect("test collector versions should be legal"),
        ),
        category,
        outcome,
    )
}

fn fingerprint_of(facets: impl IntoIterator<Item = Facet>) -> Fingerprint {
    Fingerprint::from_facets(facets).expect("test fingerprints should be valid")
}

fn parse(rendered: &str) -> Value {
    serde_json::from_str(rendered).expect("rendered output should be valid JSON")
}

/// Asserts the keys appear in this order in the rendered text.
///
/// Key order has to be checked on the bytes. Parsing into `serde_json::Value`
/// puts every object into a `BTreeMap`, so a parse-based order assertion is
/// sorted by the parser and can never fail, whatever the renderer emitted.
fn assert_keys_in_order(rendered: &str, keys: &[&str]) {
    let positions: Vec<usize> = keys
        .iter()
        .map(|key| {
            rendered
                .find(&format!("\"{key}\""))
                .unwrap_or_else(|| panic!("expected {key:?} in:\n{rendered}"))
        })
        .collect();

    let mut ascending = positions.clone();
    ascending.sort_unstable();
    assert_eq!(
        positions, ascending,
        "expected {keys:?} in that order, got offsets {positions:?} in:\n{rendered}"
    );
}

fn processes_facet(pid: i64) -> Facet {
    facet(
        "processes",
        CollectorCategory::State,
        FacetOutcome::ok(Observation::object([
            ("command", Observation::text("nginx")),
            ("pid", Observation::integer(pid).volatile()),
        ])),
    )
}

#[test]
fn a_document_leads_with_how_to_read_it_then_the_run_then_the_state() {
    // Arrange
    let fingerprint = fingerprint_of([
        facet("host", CollectorCategory::Metadata, FacetOutcome::Absent),
        facet("mounts", CollectorCategory::State, FacetOutcome::Absent),
    ]);

    // Act
    let rendered = to_canonical_json(&fingerprint, View::Complete);

    // Assert
    assert_keys_in_order(&rendered, &["schema_version", "metadata", "facets"]);
}

#[test]
fn a_facet_leads_with_the_name_a_reader_scans_for() {
    // Act
    let rendered = to_canonical_json(&fingerprint_of([processes_facet(991)]), View::Complete);

    // Assert
    assert_keys_in_order(&rendered, &["name", "collector", "status", "data"]);
}

#[test]
fn a_collector_is_identified_by_id_then_version() {
    // Act
    let rendered = to_canonical_json(&fingerprint_of([processes_facet(991)]), View::Complete);

    // Assert
    assert_keys_in_order(&rendered, &["id", "version"]);
}

#[test]
fn observed_data_has_its_keys_sorted_whatever_order_the_collector_used() {
    // Arrange: supplied in reverse alphabetical order.
    let fingerprint = fingerprint_of([facet(
        "unsorted",
        CollectorCategory::State,
        FacetOutcome::ok(Observation::object([
            ("zulu", Observation::integer(1)),
            ("mike", Observation::integer(2)),
            ("alpha", Observation::integer(3)),
        ])),
    )]);

    // Act
    let rendered = to_canonical_json(&fingerprint, View::Complete);

    // Assert: a collector's shape is not known in advance, so sorting is the
    // only fixed order available to it.
    assert_keys_in_order(&rendered, &["alpha", "mike", "zulu"]);
}

#[test]
fn to_canonical_json_orders_facets_by_name() {
    // Arrange
    let fingerprint = fingerprint_of([
        facet("processes", CollectorCategory::State, FacetOutcome::Absent),
        facet("fs", CollectorCategory::State, FacetOutcome::Absent),
    ]);

    // Act
    let document = parse(&to_canonical_json(&fingerprint, View::Complete));

    // Assert
    assert_eq!(document["facets"][0]["name"], json!("fs"));
    assert_eq!(document["facets"][1]["name"], json!("processes"));
}

#[test]
fn to_canonical_json_separates_metadata_facets_from_state_facets() {
    // Arrange
    let fingerprint = fingerprint_of([
        facet("mounts", CollectorCategory::State, FacetOutcome::Absent),
        facet("host", CollectorCategory::Metadata, FacetOutcome::Absent),
    ]);

    // Act
    let document = parse(&to_canonical_json(&fingerprint, View::Complete));

    // Assert
    assert_eq!(document["metadata"][0]["name"], json!("host"));
    assert_eq!(document["metadata"].as_array().map(Vec::len), Some(1));
    assert_eq!(document["facets"][0]["name"], json!("mounts"));
    assert_eq!(document["facets"].as_array().map(Vec::len), Some(1));
}

#[test]
fn the_complete_view_renders_volatile_values() {
    // Act
    let document = parse(&to_canonical_json(
        &fingerprint_of([processes_facet(991)]),
        View::Complete,
    ));

    // Assert
    assert_eq!(document["facets"][0]["data"]["pid"], json!(991));
}

#[test]
fn the_diffable_view_is_byte_identical_across_differing_volatile_values() {
    // Act
    let first = to_canonical_json(&fingerprint_of([processes_facet(991)]), View::Diffable);
    let second = to_canonical_json(&fingerprint_of([processes_facet(1042)]), View::Diffable);

    // Assert
    assert_eq!(first, second);
    assert!(
        !first.contains("pid"),
        "the volatile key must not reach the diffable document"
    );
}

#[test]
fn to_canonical_json_records_an_absent_facet_without_a_data_payload() {
    // Act
    let document = parse(&to_canonical_json(
        &fingerprint_of([facet(
            "nginx",
            CollectorCategory::State,
            FacetOutcome::Absent,
        )]),
        View::Complete,
    ));

    // Assert
    let facet = &document["facets"][0];
    assert_eq!(facet["status"], json!("absent"));
    assert!(facet.get("data").is_none());
    assert!(facet.get("error").is_none());
}

#[test]
fn to_canonical_json_reports_the_reason_a_facet_failed() {
    // Act
    let document = parse(&to_canonical_json(
        &fingerprint_of([facet(
            "nftables",
            CollectorCategory::State,
            FacetOutcome::error("nft exited 1: permission denied"),
        )]),
        View::Complete,
    ));

    // Assert
    let facet = &document["facets"][0];
    assert_eq!(facet["status"], json!("error"));
    assert_eq!(facet["error"], json!("nft exited 1: permission denied"));
    assert!(
        facet.get("data").is_none(),
        "a failed facet must not present a payload it never collected"
    );
}

#[test]
fn to_canonical_json_omits_data_for_a_facet_whose_payload_is_wholly_volatile() {
    // Arrange
    let fingerprint = fingerprint_of([facet(
        "uptime",
        CollectorCategory::State,
        FacetOutcome::ok(Observation::integer(84_233).volatile()),
    )]);

    // Act
    let document = parse(&to_canonical_json(&fingerprint, View::Diffable));

    // Assert
    let facet = &document["facets"][0];
    assert_eq!(facet["status"], json!("ok"));
    assert!(facet.get("data").is_none());
}

#[test]
fn to_canonical_json_ends_with_a_newline() {
    // Act
    let rendered = to_canonical_json(&fingerprint_of([]), View::Complete);

    // Assert
    assert!(rendered.ends_with("}\n"));
}
