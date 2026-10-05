//! `GET /_cluster/state/blocks`: whether the node has a master to answer cluster-wide reads.
//!
//! **Asked so a lost master is not waited out**, found by the third domain review and measured on
//! cell 28: a node that lost its master still answers `GET /` with its cluster's UUID, and each
//! cluster-wide read waits out the 30 s master timeout. Its local cluster blocks name the missing
//! master at once, block `2`, `no master`, measured on 7.17, 8.19 and 9.5.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::collectors::elasticsearch::source::HttpClient;
use crate::collectors::elasticsearch::source::json_answer::read_answer;
use crate::collectors::elasticsearch::value_objects::{HttpEndpoint, Release, Unread};

/// Before 9, `?local` keeps the read off the master the node may not have.
const LOCAL_PATH: &str = "/_cluster/state/blocks?local=true";

/// From 9, measured on 9.4.7 and 9.5.4: `?local` is deprecated, a warning the node indexes, and
/// has no effect, the answer being the node's own state either way.
const PATH: &str = "/_cluster/state/blocks";

/// The id of the block a node raises while it has no master.
const NO_MASTER_BLOCK: &str = "2";

#[derive(Deserialize)]
struct BlocksAnswer {
    blocks: Blocks,
}

#[derive(Deserialize)]
struct Blocks {
    #[serde(default)]
    global: BTreeMap<String, serde::de::IgnoredAny>,
}

/// Whether the node's own state says it has no master.
pub fn has_no_master(
    client: &HttpClient,
    endpoint: &HttpEndpoint,
    release: Release,
) -> Result<bool, Unread> {
    let path = match release.major() < 9 {
        true => LOCAL_PATH,
        false => PATH,
    };
    let answer: BlocksAnswer = read_answer(&client.get(endpoint, path)?, path)?;
    Ok(answer.blocks.global.contains_key(NO_MASTER_BLOCK))
}
