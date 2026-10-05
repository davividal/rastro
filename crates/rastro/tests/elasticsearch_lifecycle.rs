//! What will happen to data over time: ILM policies, ingest pipelines, snapshot repositories.

use rastro::collectors::elasticsearch::{ElasticsearchCollector, HttpClient};
use rastro_collector::Collector;
use rastro_fingerprint::{Completeness, Observation, Sensitivity};

mod support;

use support::es_node::{FakeNode, ROOT};
use support::observation::{field, integer, items_of, keys_of, text};

const ILM: &str = "/_ilm/policy";
const PIPELINES: &str = "/_ingest/pipeline";
const SNAPSHOTS: &str = "/_snapshot";

/// Measured on 8.15.3.
const ILM_ANSWER: &str = r#"{"app-policy":{"version":1,"modified_date":"2026-09-28T13:51:30.381Z","policy":{"phases":{"hot":{"min_age":"0ms","actions":{"rollover":{"max_age":"7d"}}},"delete":{"min_age":"30d","actions":{"delete":{"delete_searchable_snapshot":true}}}}},"in_use_by":{"indices":["myapp-tenant1_1790000000"],"data_streams":[],"composable_templates":[]}}}"#;
const PIPELINE_ANSWER: &str = r#"{"app-pipeline":{"description":"tags","processors":[{"set":{"field":"env","value":"prod"}},{"lowercase":{"field":"title"}}]}}"#;
const SNAPSHOT_ANSWER: &str = r#"{"backups":{"type":"s3","settings":{"bucket":"backups","access_key":"AKIAEXAMPLE","secret_key":"not-a-real-secret"}}}"#;

fn node_reported(routes: &[(&str, &str)], name: &str) -> Observation {
    let node = FakeNode::serving(routes);
    let proc = node.proc(name);
    let facet = ElasticsearchCollector::reading(&proc, false, HttpClient::new())
        .collect()
        .expect("a facet");
    items_of(&field(&facet, "nodes")).remove(0)
}

fn all_routes() -> Vec<(&'static str, &'static str)> {
    vec![
        ("/", ROOT),
        (ILM, ILM_ANSWER),
        (PIPELINES, PIPELINE_ANSWER),
        (SNAPSHOTS, SNAPSHOT_ANSWER),
    ]
}

#[test]
fn collect_reports_an_ilm_policy_without_the_indices_it_is_in_use_by() {
    // Act
    let reported = node_reported(&all_routes(), "elasticsearch-lifecycle-ilm");

    // Assert: `in_use_by` names the indices under the policy, which rotate; `version` already
    // records an edit, which is all `modified_date` adds.
    let policy = field(&field(&reported, "ilm_policies"), "app-policy");
    assert_eq!(keys_of(&policy), ["policy", "version"]);
    assert_eq!(integer(&field(&policy, "version")), 1);
    let delete = field(&field(&field(&policy, "policy"), "phases"), "delete");
    assert_eq!(text(&field(&delete, "min_age")), "30d");
}

#[test]
fn collect_reports_a_pipeline_with_its_processors_in_order() {
    // Act
    let reported = node_reported(&all_routes(), "elasticsearch-lifecycle-pipeline");

    // Assert: processors run in order, so the order is the pipeline.
    let pipeline = field(&field(&reported, "ingest_pipelines"), "app-pipeline");
    let processors = items_of(&field(&pipeline, "processors"));
    assert_eq!(keys_of(&processors[0]), ["set"]);
    assert_eq!(keys_of(&processors[1]), ["lowercase"]);
}

#[test]
fn collect_withholds_a_snapshot_repositorys_settings_and_keeps_its_type() {
    // Act
    let reported = node_reported(&all_routes(), "elasticsearch-lifecycle-snapshot");

    // Assert: an S3 repository not set up through the keystore carries its keys here.
    let repository = field(&field(&reported, "snapshot_repositories"), "backups");
    assert_eq!(text(&field(&repository, "type")), "s3");
    assert_eq!(
        field(&repository, "type").sensitivity(),
        Sensitivity::Public
    );
    assert_eq!(
        field(&repository, "settings").sensitivity(),
        Sensitivity::Sensitive
    );
}

#[test]
fn collect_reports_a_node_without_ilm_on_that_surface_alone() {
    // Arrange: an OSS build of 7.x has no ILM and answers 400 to the request.
    let routes = vec![
        ("/", ROOT),
        (PIPELINES, PIPELINE_ANSWER),
        (SNAPSHOTS, SNAPSHOT_ANSWER),
    ];

    // Act
    let reported = node_reported(&routes, "elasticsearch-lifecycle-no-ilm");

    // Assert
    let policies = field(&reported, "ilm_policies");
    assert_eq!(policies.completeness(), Completeness::Incomplete);
    assert_eq!(
        keys_of(&field(&reported, "ingest_pipelines")),
        ["app-pipeline"]
    );
}

#[test]
fn collect_withholds_each_ingest_pipeline_and_keeps_its_name() {
    // Arrange: found by review. A pipeline can carry a token, a `set` processor writing an
    // `Authorization` header say, so each one is withheld on its own: a change to one pipeline is
    // a change to one digest, and which pipelines exist stays readable.
    let reported = node_reported(&all_routes(), "elasticsearch-lifecycle-pipeline-withheld");

    // Act
    let pipelines = field(&reported, "ingest_pipelines");

    // Assert
    assert_eq!(keys_of(&pipelines), ["app-pipeline"]);
    assert_eq!(pipelines.sensitivity(), Sensitivity::Public);
    assert_eq!(
        field(&pipelines, "app-pipeline").sensitivity(),
        Sensitivity::Sensitive
    );
}

#[test]
fn collect_reports_a_node_with_no_pipeline_as_holding_none() {
    // Arrange: found by review, measured on 7.17.29 once its two built-in pipelines were deleted:
    // `GET /_ingest/pipeline` answers 404 with `{}` where there is none, and the node read as an
    // error for an empty, healthy surface.
    let node = FakeNode::answering(&[("/", 200, ROOT), (PIPELINES, 404, "{}")]);
    let proc = node.proc("elasticsearch-lifecycle-no-pipeline");

    // Act
    let facet = ElasticsearchCollector::reading(&proc, false, HttpClient::new())
        .collect()
        .expect("a facet");

    // Assert
    let reported = items_of(&field(&facet, "nodes")).remove(0);
    assert!(
        keys_of(&field(&reported, "ingest_pipelines")).is_empty(),
        "{reported:?}"
    );
}
