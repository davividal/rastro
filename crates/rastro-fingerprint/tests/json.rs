use rastro_fingerprint::Observation;
use rastro_fingerprint::View;
use rastro_fingerprint::json::{to_canonical_json, to_canonical_json_writer};
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

/// Every shape a collector can hand the renderer, in one facet.
///
/// Deliberately exhaustive over `Content` and `Scalar` plus both ways a view drops a value,
/// because this is the fixture behind the golden-bytes test below.
fn every_shape() -> Observation {
    Observation::object([
        ("absent", Observation::null()),
        ("enabled", Observation::boolean(true)),
        ("count", Observation::integer(-7)),
        ("name", Observation::text("keep \"me\"\n")),
        (
            "nested",
            Observation::object([
                ("inner", Observation::text("deep")),
                ("dropped_leaf", Observation::integer(1).volatile()),
            ]),
        ),
        (
            "items",
            Observation::list([
                Observation::integer(1),
                Observation::text("two"),
                Observation::null(),
            ]),
        ),
        (
            "dropped_subtree",
            Observation::object([("gone", Observation::text("with it"))]).volatile(),
        ),
        ("secret", Observation::text("password=hunter2").sensitive()),
    ])
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

#[test]
fn to_canonical_json_renders_this_exact_document() {
    // Arrange: every `Content` and `Scalar` variant, a nested object, a list, a volatile leaf
    // and a wholly volatile subtree. Pinned to the byte because the format *is* the contract
    // and the determinism harness compares raw bytes — so any later change to how the tree is
    // serialised has to prove itself here rather than be argued about.
    let document = fingerprint_of([facet(
        "shapes",
        CollectorCategory::State,
        FacetOutcome::Ok {
            observation: every_shape(),
        },
    )]);

    // Act
    let rendered = to_canonical_json(&document, View::Diffable);

    // Assert
    assert_eq!(
        rendered,
        r#"{
  "schema_version": 1,
  "metadata": [],
  "facets": [
    {
      "name": "shapes",
      "collector": {
        "id": "shapes",
        "version": "1"
      },
      "status": "ok",
      "data": {
        "absent": null,
        "count": -7,
        "enabled": true,
        "items": [
          1,
          "two",
          null
        ],
        "name": "keep \"me\"\n",
        "nested": {
          "inner": "deep"
        },
        "secret": "redacted:sha256+xxh3:afdfd5279de51e0f"
      }
    }
  ]
}
"#
    );
}

#[test]
fn to_canonical_json_renders_this_exact_document_in_the_complete_view() {
    // Arrange: the same fixture, so the two goldens together pin what a view changes and
    // nothing else. `View::Complete` pays a full clone today for zero benefit, and this is
    // what will prove that removing the clone changed no byte.
    let document = fingerprint_of([facet(
        "shapes",
        CollectorCategory::State,
        FacetOutcome::Ok {
            observation: every_shape(),
        },
    )]);

    // Act
    let rendered = to_canonical_json(&document, View::Complete);

    // Assert
    assert_eq!(
        rendered,
        r#"{
  "schema_version": 1,
  "metadata": [],
  "facets": [
    {
      "name": "shapes",
      "collector": {
        "id": "shapes",
        "version": "1"
      },
      "status": "ok",
      "data": {
        "absent": null,
        "count": -7,
        "dropped_subtree": {
          "gone": "with it"
        },
        "enabled": true,
        "items": [
          1,
          "two",
          null
        ],
        "name": "keep \"me\"\n",
        "nested": {
          "dropped_leaf": 1,
          "inner": "deep"
        },
        "secret": "redacted:sha256+xxh3:afdfd5279de51e0f"
      }
    }
  ]
}
"#
    );
}

#[test]
fn to_canonical_json_writer_and_to_canonical_json_agree_byte_for_byte() {
    // Arrange: the same fixture the golden tests pin, so this compares the two paths against
    // each other and the goldens pin what they both produce.
    let document = fingerprint_of([facet(
        "shapes",
        CollectorCategory::State,
        FacetOutcome::Ok {
            observation: every_shape(),
        },
    )]);

    for view in [View::Diffable, View::Complete] {
        // Act
        let mut streamed = Vec::new();
        to_canonical_json_writer(&document, view, &mut streamed).expect("a Vec accepts every byte");

        // Assert: a document of tens of thousands of entries should not exist twice in memory
        // just to be written, and the way to be sure that change cost nothing is to compare
        // the bytes rather than to reason about them.
        assert_eq!(
            String::from_utf8(streamed).expect("the document is UTF-8"),
            to_canonical_json(&document, view),
            "{view:?}"
        );
    }
}

#[test]
fn to_canonical_json_writer_reports_a_write_that_failed() {
    // Arrange: a writer that refuses, standing in for the disk filling up half way through a
    // fingerprint. The caller names the path, because this module has no business knowing it.
    struct Refuses;
    impl std::io::Write for Refuses {
        fn write(&mut self, _buffer: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("no space left on device"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    // Act
    let refused = to_canonical_json_writer(
        &fingerprint_of([processes_facet(991)]),
        View::Diffable,
        &mut Refuses,
    );

    // Assert
    assert!(refused.is_err());
}
