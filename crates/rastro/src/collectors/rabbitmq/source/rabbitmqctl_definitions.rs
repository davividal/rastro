//! The `rabbitmqctl export_definitions -` interface.
//!
//! One call for the whole durable half of a node: its vhosts, its users, their permissions,
//! the policies and parameters, and the topology somebody declared. The fat command the
//! footprint measurement argued for, at 254 ms for what would otherwise be ten CLI
//! invocations of an Erlang VM each.
//!
//! **`-` rather than a file name**, which is the difference between a read and a write: given
//! a path, this command creates the file.
//!
//! **The export carries credential material**, and this module is where it enters rastro.
//! Nothing here logs a user record or renders one into an error, because the material cannot
//! be unpublished once a document carries it.

use serde::Deserialize;

use rastro_collector::CollectionError;

use crate::collectors::rabbitmq::model::{Definitions, User, UserLimit, Vhost};
use crate::collectors::rabbitmq::value_objects::PasswordHashing;

/// The subset of the export rastro reads, spelled as RabbitMQ spells it.
///
/// Unknown fields are ignored: RabbitMQ adds to this document between releases, and refusing
/// an unfamiliar key would fail the facet on the next upgrade of a box nobody changed.
#[derive(Debug, Deserialize)]
struct DefinitionsDocument {
    rabbitmq_version: Option<String>,
    #[serde(default)]
    vhosts: Vec<VhostDocument>,
    #[serde(default)]
    users: Vec<UserDocument>,
}

#[derive(Debug, Deserialize)]
struct VhostDocument {
    name: Option<String>,
    default_queue_type: Option<String>,
    metadata: Option<VhostMetadata>,
}

#[derive(Debug, Deserialize)]
struct VhostMetadata {
    description: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct UserDocument {
    name: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    hashing_algorithm: Option<String>,
    password_hash: Option<String>,
    #[serde(default)]
    limits: std::collections::BTreeMap<String, i64>,
}

/// What a node exports of its own definitions.
pub struct RabbitmqctlDefinitions;

impl RabbitmqctlDefinitions {
    /// Reads one node's definitions export.
    ///
    /// **A document with no `rabbitmq_version` is refused**, on the same reasoning the status
    /// read refuses one: the register names every Erlang node on the box, and an answer that
    /// does not say which RabbitMQ wrote it is not evidence about a broker.
    pub fn parse(output: &str) -> Result<Definitions, CollectionError> {
        let document: DefinitionsDocument = serde_json::from_str(output).map_err(|failure| {
            CollectionError::new(format!(
                "rabbitmqctl export_definitions did not answer with a JSON document, so this \
                 node's definitions could not be read: {failure}"
            ))
        })?;

        let rabbitmq_version = document
            .rabbitmq_version
            .filter(|version| !version.is_empty());
        if rabbitmq_version.is_none() {
            return Err(CollectionError::new(
                "a definitions document carrying no \"rabbitmq_version\" did not come from a \
                 RabbitMQ node, so nothing in it can be recorded as one's state",
            ));
        }

        Ok(Definitions {
            rabbitmq_version,
            vhosts: document.vhosts.into_iter().filter_map(vhost_of).collect(),
            users: document.users.into_iter().filter_map(user_of).collect(),
        })
    }
}

/// One vhost, keyed by its name, or nothing where the export named none.
///
/// A nameless entry is dropped rather than failing the read: it can only come from a
/// RabbitMQ that changed the document's shape, and one unnameable vhost is not worth every
/// other vhost, user and policy in the export.
fn vhost_of(document: VhostDocument) -> Option<(String, Vhost)> {
    let name = document.name?;
    let metadata = document.metadata;

    Some((
        name,
        Vhost {
            default_queue_type: document.default_queue_type,
            description: metadata
                .as_ref()
                .and_then(|metadata| metadata.description.clone())
                .filter(|description| !description.is_empty()),
            tags: metadata.map(|metadata| metadata.tags).unwrap_or_default(),
        },
    ))
}

/// One user, keyed by its name, with its verifier kept only where its scheme allows.
fn user_of(document: UserDocument) -> Option<(String, User)> {
    let name = document.name?;
    let hashing = document
        .hashing_algorithm
        .as_deref()
        .map(PasswordHashing::parse);

    // The verifier is dropped here, at the boundary, rather than filtered at render time.
    // A value the model never holds cannot be printed by a later mistake in a renderer.
    let password_hash = match &hashing {
        Some(hashing) if hashing.carries_verifier() => {
            document.password_hash.filter(|hash| !hash.is_empty())
        }
        _ => None,
    };

    let limits = document
        .limits
        .into_iter()
        .map(|(name, value)| UserLimit { name, value })
        .collect();

    Some((
        name,
        User {
            tags: document.tags,
            password_hashing: hashing,
            password_hash,
            limits,
        },
    ))
}
