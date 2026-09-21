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

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;

use rastro_collector::CollectionError;

use crate::collectors::rabbitmq::model::{
    Binding, Definitions, Exchange, Parameter, Permission, Policy, Queue, TopicPermission, User,
    UserLimit, Vhost,
};
use crate::collectors::rabbitmq::value_objects::DefinitionValue;
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
    #[serde(default)]
    permissions: Vec<PermissionDocument>,
    #[serde(default)]
    topic_permissions: Vec<TopicPermissionDocument>,
    #[serde(default)]
    policies: Vec<PolicyDocument>,
    #[serde(default)]
    parameters: Vec<ParameterDocument>,
    #[serde(default)]
    global_parameters: Vec<GlobalParameterDocument>,
    #[serde(default)]
    exchanges: Vec<ExchangeDocument>,
    #[serde(default)]
    queues: Vec<QueueDocument>,
    #[serde(default)]
    bindings: Vec<BindingDocument>,
}

#[derive(Debug, Deserialize)]
struct ExchangeDocument {
    name: Option<String>,
    vhost: Option<String>,
    #[serde(rename = "type")]
    exchange_type: Option<String>,
    #[serde(default)]
    durable: bool,
    #[serde(default)]
    auto_delete: bool,
    #[serde(default)]
    arguments: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct QueueDocument {
    name: Option<String>,
    vhost: Option<String>,
    #[serde(rename = "type")]
    queue_type: Option<String>,
    #[serde(default)]
    durable: bool,
    #[serde(default)]
    auto_delete: bool,
    #[serde(default)]
    arguments: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct BindingDocument {
    vhost: Option<String>,
    source: Option<String>,
    destination: Option<String>,
    destination_type: Option<String>,
    routing_key: Option<String>,
    #[serde(default)]
    arguments: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct PolicyDocument {
    name: Option<String>,
    vhost: Option<String>,
    pattern: Option<String>,
    #[serde(rename = "apply-to")]
    apply_to: Option<String>,
    priority: Option<i64>,
    #[serde(default)]
    definition: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct ParameterDocument {
    name: Option<String>,
    vhost: Option<String>,
    component: Option<String>,
    value: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct GlobalParameterDocument {
    name: Option<String>,
    value: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct PermissionDocument {
    user: Option<String>,
    vhost: Option<String>,
    configure: Option<String>,
    write: Option<String>,
    read: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TopicPermissionDocument {
    user: Option<String>,
    vhost: Option<String>,
    exchange: Option<String>,
    write: Option<String>,
    read: Option<String>,
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
            permissions: permissions_of(document.permissions),
            topic_permissions: topic_permissions_of(document.topic_permissions),
            policies: policies_of(document.policies),
            parameters: parameters_of(document.parameters),
            global_parameters: document
                .global_parameters
                .into_iter()
                .filter_map(|parameter| Some((parameter.name?, parameter_of(parameter.value))))
                .collect(),
            exchanges: exchanges_of(document.exchanges),
            queues: queues_of(document.queues),
            bindings: bindings_of(document.bindings),
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

/// The permissions, nested by vhost and then user.
///
/// An entry naming neither is dropped rather than failing the read, on the same reasoning a
/// nameless vhost is: it can only come from a RabbitMQ that changed the document's shape, and
/// one unplaceable grant is not worth every other grant in the export. A dropped entry is
/// visible as a permission that is simply not there, which is what an operator would then
/// investigate.
fn permissions_of(
    documents: Vec<PermissionDocument>,
) -> BTreeMap<String, BTreeMap<String, Permission>> {
    let mut permissions: BTreeMap<String, BTreeMap<String, Permission>> = BTreeMap::new();

    for document in documents {
        let (Some(vhost), Some(user)) = (document.vhost, document.user) else {
            continue;
        };

        permissions.entry(vhost).or_default().insert(
            user,
            Permission {
                configure: document.configure.unwrap_or_default(),
                write: document.write.unwrap_or_default(),
                read: document.read.unwrap_or_default(),
            },
        );
    }

    permissions
}

/// The topic permissions, nested by vhost, user and exchange.
fn topic_permissions_of(
    documents: Vec<TopicPermissionDocument>,
) -> BTreeMap<String, BTreeMap<String, BTreeMap<String, TopicPermission>>> {
    let mut permissions: BTreeMap<String, BTreeMap<String, BTreeMap<String, TopicPermission>>> =
        BTreeMap::new();

    for document in documents {
        let (Some(vhost), Some(user), Some(exchange)) =
            (document.vhost, document.user, document.exchange)
        else {
            continue;
        };

        permissions
            .entry(vhost)
            .or_default()
            .entry(user)
            .or_default()
            .insert(
                exchange,
                TopicPermission {
                    write: document.write.unwrap_or_default(),
                    read: document.read.unwrap_or_default(),
                },
            );
    }

    permissions
}

/// The policies, nested by vhost and then name.
fn policies_of(documents: Vec<PolicyDocument>) -> BTreeMap<String, BTreeMap<String, Policy>> {
    let mut policies: BTreeMap<String, BTreeMap<String, Policy>> = BTreeMap::new();

    for document in documents {
        let (Some(vhost), Some(name)) = (document.vhost, document.name) else {
            continue;
        };

        policies.entry(vhost).or_default().insert(
            name,
            Policy {
                pattern: document.pattern.unwrap_or_default(),
                apply_to: document.apply_to,
                priority: document.priority,
                definition: document
                    .definition
                    .into_iter()
                    .map(|(key, value)| (key, definition_value_of(&value)))
                    .collect(),
            },
        );
    }

    policies
}

/// The parameters, nested by vhost, component and name.
fn parameters_of(
    documents: Vec<ParameterDocument>,
) -> BTreeMap<String, BTreeMap<String, BTreeMap<String, Parameter>>> {
    let mut parameters: BTreeMap<String, BTreeMap<String, BTreeMap<String, Parameter>>> =
        BTreeMap::new();

    for document in documents {
        let (Some(vhost), Some(component), Some(name)) =
            (document.vhost, document.component, document.name)
        else {
            continue;
        };

        parameters
            .entry(vhost)
            .or_default()
            .entry(component)
            .or_default()
            .insert(name, parameter_of(document.value));
    }

    parameters
}

/// A parameter, carrying its value's own JSON spelling and nothing interpreted.
///
/// Compact rather than pretty, so that the same parameter renders the same bytes whatever
/// the exporter's own formatting does.
fn parameter_of(value: Option<serde_json::Value>) -> Parameter {
    Parameter {
        value: match value {
            Some(value) => value.to_string(),
            None => String::new(),
        },
    }
}

/// One definition value in the three shapes the document can carry.
///
/// A number that is not an integer keeps its own spelling as text, because the format admits
/// no floating point and rounding one would report a policy the broker does not have. A
/// nested shape does the same, for a shape nobody has measured.
fn definition_value_of(value: &serde_json::Value) -> DefinitionValue {
    match value {
        serde_json::Value::Bool(flag) => DefinitionValue::Boolean(*flag),
        serde_json::Value::Number(number) => match number.as_i64() {
            Some(integer) => DefinitionValue::Integer(integer),
            None => DefinitionValue::Text(number.to_string()),
        },
        serde_json::Value::String(text) => DefinitionValue::Text(text.clone()),
        other => DefinitionValue::Text(other.to_string()),
    }
}

/// The durable exchanges, nested by vhost and then name.
fn exchanges_of(documents: Vec<ExchangeDocument>) -> BTreeMap<String, BTreeMap<String, Exchange>> {
    let mut exchanges: BTreeMap<String, BTreeMap<String, Exchange>> = BTreeMap::new();

    for document in documents {
        let (Some(vhost), Some(name)) = (document.vhost, document.name) else {
            continue;
        };

        exchanges.entry(vhost).or_default().insert(
            name,
            Exchange {
                exchange_type: document.exchange_type.unwrap_or_default(),
                durable: document.durable,
                auto_delete: document.auto_delete,
                arguments: arguments_of(document.arguments),
            },
        );
    }

    exchanges
}

/// The durable queues, nested by vhost and then name.
fn queues_of(documents: Vec<QueueDocument>) -> BTreeMap<String, BTreeMap<String, Queue>> {
    let mut queues: BTreeMap<String, BTreeMap<String, Queue>> = BTreeMap::new();

    for document in documents {
        let (Some(vhost), Some(name)) = (document.vhost, document.name) else {
            continue;
        };

        queues.entry(vhost).or_default().insert(
            name,
            Queue {
                queue_type: document.queue_type.unwrap_or_default(),
                durable: document.durable,
                auto_delete: document.auto_delete,
                arguments: arguments_of(document.arguments),
            },
        );
    }

    queues
}

/// The bindings of each vhost, as sets that order themselves.
fn bindings_of(documents: Vec<BindingDocument>) -> BTreeMap<String, BTreeSet<Binding>> {
    let mut bindings: BTreeMap<String, BTreeSet<Binding>> = BTreeMap::new();

    for document in documents {
        let Some(vhost) = document.vhost else {
            continue;
        };

        bindings.entry(vhost).or_default().insert(Binding {
            source: document.source.unwrap_or_default(),
            destination_type: document.destination_type.unwrap_or_default(),
            destination: document.destination.unwrap_or_default(),
            routing_key: document.routing_key.unwrap_or_default(),
            arguments: arguments_of(document.arguments),
        });
    }

    bindings
}

/// The `x-` arguments of a declaration, in rastro's own value vocabulary.
fn arguments_of(
    arguments: BTreeMap<String, serde_json::Value>,
) -> BTreeMap<String, DefinitionValue> {
    arguments
        .into_iter()
        .map(|(name, value)| (name, definition_value_of(&value)))
        .collect()
}
