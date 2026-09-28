//! `GET /`: which node this is, of which cluster, running what.

use serde::Deserialize;

use crate::collectors::elasticsearch::model::{NodeIdentity, NodeVersion};
use crate::collectors::elasticsearch::source::HttpClient;
use crate::collectors::elasticsearch::source::json_answer::read_answer;
use crate::collectors::elasticsearch::value_objects::{HttpEndpoint, Unread};

const PATH: &str = "/";

/// The answer as the node spells it. `build_date` and `lucene_version` follow from the build
/// and are left out, `tagline` is a slogan.
#[derive(Deserialize)]
struct RootAnswer {
    name: String,
    cluster_name: String,
    cluster_uuid: String,
    version: VersionAnswer,
}

#[derive(Deserialize)]
struct VersionAnswer {
    number: String,
    build_flavor: Option<String>,
    build_type: Option<String>,
    build_hash: Option<String>,
}

/// Asks the node who it is.
pub fn read_identity(client: &HttpClient, endpoint: &HttpEndpoint) -> Result<NodeIdentity, Unread> {
    let answer: RootAnswer = read_answer(&client.get(endpoint, PATH)?, PATH)?;

    Ok(NodeIdentity {
        node_name: answer.name,
        cluster_name: answer.cluster_name,
        cluster_uuid: answer.cluster_uuid,
        version: NodeVersion {
            number: answer.version.number,
            build_flavor: answer.version.build_flavor,
            build_type: answer.version.build_type,
            build_hash: answer.version.build_hash,
        },
    })
}
