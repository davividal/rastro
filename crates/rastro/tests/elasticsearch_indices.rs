//! Indices, keyed so that a rotation which changes nothing reads as nothing changed.
//!
//! The field host's indices are named `<app>-<tenant>_<epoch>` and rebuilt under a new name
//! behind a stable alias. Keyed by the index name, every rebuild would read as one index
//! removed and another added. Keyed by the alias, it reads as the volatile fields moving and
//! the schema staying, or as the schema changing, which is the one thing worth seeing.

use rastro::collectors::elasticsearch::{ElasticsearchCollector, HttpClient};
use rastro_collector::Collector;
use rastro_fingerprint::{Observation, Volatility};

mod support;

use support::es_node::{FakeNode, ROOT};
use support::observation::{field, items_of, keys_of, text};

const ALIASES: &str = "/*/_alias?expand_wildcards=open,closed";
const SETTINGS: &str = "/*/_settings?flat_settings=true&expand_wildcards=open,closed";
const MAPPINGS: &str = "/*/_mapping?expand_wildcards=open,closed";

/// Measured on 8.15.3.
const ALIAS_ANSWER: &str =
    r#"{"myapp-tenant1_1790000000":{"aliases":{"myapp-tenant1":{}}},"unaliased":{"aliases":{}}}"#;

fn settings_answer(index: &str, uuid: &str, created: &str) -> String {
    format!(
        r#""{index}":{{"settings":{{"index.creation_date":"{created}","index.number_of_replicas":"0","index.number_of_shards":"1","index.provided_name":"{index}","index.routing.allocation.include._tier_preference":"data_content","index.uuid":"{uuid}","index.version.created":"8512000"}}}}"#
    )
}

fn settings_of(entries: &[String]) -> String {
    format!("{{{}}}", entries.join(","))
}

const MAPPING_ANSWER: &str = r#"{"myapp-tenant1_1790000000":{"mappings":{"properties":{"score":{"type":"float"},"title":{"type":"text"}}}},"unaliased":{"mappings":{}}}"#;

fn indices_of(routes: &[(&str, &str)], name: &str) -> Observation {
    let node = FakeNode::serving(routes);
    let proc = node.proc(name);
    let facet = ElasticsearchCollector::reading(&proc, false, HttpClient::new())
        .collect()
        .expect("a facet");
    field(&items_of(&field(&facet, "nodes"))[0], "indices")
}

#[test]
fn collect_keys_an_aliased_index_by_its_alias_and_an_unaliased_one_by_its_name() {
    // Arrange
    let settings = settings_of(&[
        settings_answer(
            "myapp-tenant1_1790000000",
            "RmiWDuNDRKO0ODXyykEWaQ",
            "1790603490253",
        ),
        settings_answer("unaliased", "DqUoBesrSJGb7PgJWKGoaA", "1790603490341"),
    ]);

    // Act
    let indices = indices_of(
        &[
            ("/", ROOT),
            (ALIASES, ALIAS_ANSWER),
            (SETTINGS, &settings),
            (MAPPINGS, MAPPING_ANSWER),
        ],
        "elasticsearch-indices-keys",
    );

    // Assert
    assert_eq!(keys_of(&indices), ["myapp-tenant1", "unaliased"]);
    let aliased = field(&indices, "myapp-tenant1");
    assert_eq!(text(&field(&aliased, "index")), "myapp-tenant1_1790000000");
    assert_eq!(field(&aliased, "index").volatility(), Volatility::Volatile);
    assert_eq!(
        field(&field(&indices, "unaliased"), "index").volatility(),
        Volatility::Stable
    );
}

#[test]
fn collect_records_the_stable_settings_and_marks_the_per_index_ones_volatile() {
    // Arrange
    let settings = settings_of(&[
        settings_answer(
            "myapp-tenant1_1790000000",
            "RmiWDuNDRKO0ODXyykEWaQ",
            "1790603490253",
        ),
        settings_answer("unaliased", "DqUoBesrSJGb7PgJWKGoaA", "1790603490341"),
    ]);

    // Act
    let indices = indices_of(
        &[
            ("/", ROOT),
            (ALIASES, ALIAS_ANSWER),
            (SETTINGS, &settings),
            (MAPPINGS, MAPPING_ANSWER),
        ],
        "elasticsearch-indices-settings",
    );

    // Assert: `version.created` is kept, since it records which release made the index.
    let aliased = field(&indices, "myapp-tenant1");
    assert_eq!(
        keys_of(&field(&aliased, "settings")),
        [
            "index.number_of_replicas",
            "index.number_of_shards",
            "index.routing.allocation.include._tier_preference",
            "index.version.created",
        ]
    );
    assert_eq!(text(&field(&aliased, "uuid")), "RmiWDuNDRKO0ODXyykEWaQ");
    assert_eq!(field(&aliased, "uuid").volatility(), Volatility::Volatile);
    assert_eq!(
        field(&aliased, "creation_date").volatility(),
        Volatility::Volatile
    );
}

#[test]
fn collect_digests_a_rotated_index_with_the_same_schema_to_the_same_value() {
    // Arrange: the same index rebuilt under a new name, uuid and creation date.
    let before = settings_of(&[settings_answer(
        "myapp-tenant1_1790000000",
        "RmiWDuNDRKO0ODXyykEWaQ",
        "1790603490253",
    )]);
    let after = settings_of(&[settings_answer(
        "myapp-tenant1_1790099999",
        "Zx1b2c3d4e5f6g7h8i9j0k",
        "1790700000000",
    )]);
    let mapping = |index: &str| {
        format!(
            r#"{{"{index}":{{"mappings":{{"properties":{{"title":{{"type":"text"}},"score":{{"type":"float"}}}}}}}}}}"#
        )
    };
    let alias = |index: &str| format!(r#"{{"{index}":{{"aliases":{{"myapp-tenant1":{{}}}}}}}}"#);
    let (old_index, new_index) = ("myapp-tenant1_1790000000", "myapp-tenant1_1790099999");

    // Act
    let first = indices_of(
        &[
            ("/", ROOT),
            (ALIASES, &alias(old_index)),
            (SETTINGS, &before),
            (MAPPINGS, &mapping(old_index)),
        ],
        "elasticsearch-indices-rotation-before",
    );
    let second = indices_of(
        &[
            ("/", ROOT),
            (ALIASES, &alias(new_index)),
            (SETTINGS, &after),
            (MAPPINGS, &mapping(new_index)),
        ],
        "elasticsearch-indices-rotation-after",
    );

    // Assert: in the diffable view the two are one entry that did not change.
    let digest =
        |indices: &Observation| text(&field(&field(indices, "myapp-tenant1"), "mappings_digest"));
    assert_eq!(digest(&first), digest(&second));
    assert_eq!(
        field(&field(&first, "myapp-tenant1"), "settings"),
        field(&field(&second, "myapp-tenant1"), "settings")
    );
}

#[test]
fn collect_digests_a_changed_mapping_to_a_different_value() {
    // Arrange
    let settings = settings_of(&[settings_answer(
        "myapp-tenant1_1790000000",
        "RmiWDuNDRKO0ODXyykEWaQ",
        "1790603490253",
    )]);
    let alias = r#"{"myapp-tenant1_1790000000":{"aliases":{"myapp-tenant1":{}}}}"#;
    let text_field =
        r#"{"myapp-tenant1_1790000000":{"mappings":{"properties":{"title":{"type":"text"}}}}}"#;
    let keyword_field =
        r#"{"myapp-tenant1_1790000000":{"mappings":{"properties":{"title":{"type":"keyword"}}}}}"#;

    // Act
    let first = indices_of(
        &[
            ("/", ROOT),
            (ALIASES, alias),
            (SETTINGS, &settings),
            (MAPPINGS, text_field),
        ],
        "elasticsearch-indices-mapping-before",
    );
    let second = indices_of(
        &[
            ("/", ROOT),
            (ALIASES, alias),
            (SETTINGS, &settings),
            (MAPPINGS, keyword_field),
        ],
        "elasticsearch-indices-mapping-after",
    );

    // Assert
    let digest =
        |indices: &Observation| text(&field(&field(indices, "myapp-tenant1"), "mappings_digest"));
    assert_ne!(digest(&first), digest(&second));
}

#[test]
fn collect_keys_by_name_where_an_alias_spans_several_indices() {
    // Arrange: a read alias over two generations names neither of them alone.
    let aliases = r#"{"logs-1":{"aliases":{"logs":{}}},"logs-2":{"aliases":{"logs":{}}}}"#;
    let settings = settings_of(&[
        settings_answer("logs-1", "aaaaaaaaaaaaaaaaaaaaaa", "1"),
        settings_answer("logs-2", "bbbbbbbbbbbbbbbbbbbbbb", "2"),
    ]);
    let mappings = r#"{"logs-1":{"mappings":{}},"logs-2":{"mappings":{}}}"#;

    // Act
    let indices = indices_of(
        &[
            ("/", ROOT),
            (ALIASES, aliases),
            (SETTINGS, &settings),
            (MAPPINGS, mappings),
        ],
        "elasticsearch-indices-shared-alias",
    );

    // Assert
    assert_eq!(keys_of(&indices), ["logs-1", "logs-2"]);
    assert_eq!(
        keys_of(&field(&field(&indices, "logs-1"), "aliases")),
        ["logs"]
    );
}

#[test]
fn collect_keys_by_name_where_an_index_has_several_aliases() {
    // Arrange: a read and a write alias on one index; picking one would be a guess.
    let aliases = r#"{"orders-7":{"aliases":{"orders-write":{},"orders":{}}}}"#;
    let settings = settings_of(&[settings_answer("orders-7", "cccccccccccccccccccccc", "3")]);
    let mappings = r#"{"orders-7":{"mappings":{}}}"#;

    // Act
    let indices = indices_of(
        &[
            ("/", ROOT),
            (ALIASES, aliases),
            (SETTINGS, &settings),
            (MAPPINGS, mappings),
        ],
        "elasticsearch-indices-two-aliases",
    );

    // Assert
    assert_eq!(keys_of(&indices), ["orders-7"]);
    assert_eq!(
        keys_of(&field(&field(&indices, "orders-7"), "aliases")),
        ["orders", "orders-write"]
    );
}

#[test]
fn collect_leaves_out_the_backing_index_of_a_data_stream() {
    // Arrange: measured on 8.15.3 and 9.2.0 by the domain review. `expand_wildcards=open,closed`
    // is applied to the data stream, which is not hidden, and the stream then expands to its
    // backing indices, which are. So the answer carries them, flagged only by their own
    // `index.hidden`, and a plain hidden index is the control that the same query does leave out.
    let backing = ".ds-logs-myapp-default-2026.09.28-000001";
    let aliases = format!(r#"{{"unaliased":{{"aliases":{{}}}},"{backing}":{{"aliases":{{}}}}}}"#);
    let settings = format!(
        r#"{{{},"{backing}":{{"settings":{{"index.hidden":"true","index.number_of_shards":"1","index.uuid":"dddddddddddddddddddddd","index.creation_date":"4","index.provided_name":"{backing}"}}}}}}"#,
        settings_answer("unaliased", "DqUoBesrSJGb7PgJWKGoaA", "1790603490341")
    );
    let mappings =
        format!(r#"{{"unaliased":{{"mappings":{{}}}},"{backing}":{{"mappings":{{}}}}}}"#);

    // Act
    let indices = indices_of(
        &[
            ("/", ROOT),
            (ALIASES, &aliases),
            (SETTINGS, &settings),
            (MAPPINGS, &mappings),
        ],
        "elasticsearch-indices-backing",
    );

    // Assert: a rollover would otherwise read as one index removed and another added.
    assert_eq!(keys_of(&indices), ["unaliased"]);
}

#[test]
fn collect_sees_a_rollover_move_the_write_index() {
    // Arrange: found by review. Only alias names were kept, so moving `is_write_index` between
    // the two generations behind a rollover alias, or changing an alias's filter, read as nothing
    // changed, though where documents are written had.
    let settings = settings_of(&[
        settings_answer("logs-1", "aaaaaaaaaaaaaaaaaaaaaa", "1"),
        settings_answer("logs-2", "bbbbbbbbbbbbbbbbbbbbbb", "2"),
    ]);
    let mappings = r#"{"logs-1":{"mappings":{}},"logs-2":{"mappings":{}}}"#;
    let writing_to = |first: bool| {
        format!(
            r#"{{"logs-1":{{"aliases":{{"logs":{{"is_write_index":{first}}}}}}},"logs-2":{{"aliases":{{"logs":{{"is_write_index":{}}}}}}}}}"#,
            !first
        )
    };

    // Act
    let before = indices_of(
        &[
            ("/", ROOT),
            (ALIASES, &writing_to(true)),
            (SETTINGS, &settings),
            (MAPPINGS, mappings),
        ],
        "elasticsearch-indices-write-before",
    );
    let after = indices_of(
        &[
            ("/", ROOT),
            (ALIASES, &writing_to(false)),
            (SETTINGS, &settings),
            (MAPPINGS, mappings),
        ],
        "elasticsearch-indices-write-after",
    );

    // Assert
    let writes = |indices: &Observation, index: &str| {
        support::observation::boolean(&field(
            &field(&field(&field(indices, index), "aliases"), "logs"),
            "is_write_index",
        ))
    };
    assert!(writes(&before, "logs-1") && !writes(&before, "logs-2"));
    assert!(!writes(&after, "logs-1") && writes(&after, "logs-2"));
}

#[test]
fn collect_keeps_an_aliases_filter() {
    // Arrange: a filtered alias is a query; changing the filter changes what it returns.
    let aliases = r#"{"orders-7":{"aliases":{"paid":{"filter":{"term":{"status":"paid"}}}}}}"#;
    let settings = settings_of(&[settings_answer("orders-7", "cccccccccccccccccccccc", "3")]);
    let mappings = r#"{"orders-7":{"mappings":{}}}"#;

    // Act
    let indices = indices_of(
        &[
            ("/", ROOT),
            (ALIASES, aliases),
            (SETTINGS, &settings),
            (MAPPINGS, mappings),
        ],
        "elasticsearch-indices-filtered-alias",
    );

    // Assert: keyed by the alias, which is its identity, and the filter kept on it.
    let paid = field(&field(&field(&indices, "paid"), "aliases"), "paid");
    let term = field(&field(&paid, "filter"), "term");
    assert_eq!(text(&field(&term, "status")), "paid");
}
