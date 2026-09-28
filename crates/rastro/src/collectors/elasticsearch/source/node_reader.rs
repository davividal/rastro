//! One node, read in the only order that asks nothing blind.
//!
//! Settings, then whether they allow plain HTTP, then the namespace, then the listeners, then
//! which of them serves HTTP, and only then a request, made inside the node's namespace. Each
//! step's refusal stops the read there, and what the earlier steps established is kept.

use std::path::Path;

use crate::collectors::elasticsearch::model::{
    ClusterSettings, Indices, NamedDefinitions, Node, NodeIdentity, Surface,
};
use crate::collectors::elasticsearch::source::cluster_settings_answer::read_cluster_settings;
use crate::collectors::elasticsearch::source::indices_answer::read_indices;
use crate::collectors::elasticsearch::source::root_answer::read_identity;
use crate::collectors::elasticsearch::source::templates_answer::{
    read_component_templates, read_index_templates,
};
use crate::collectors::elasticsearch::source::{
    HttpClient, NodeListener, NodeNamespace, NodeSettings, ResidentNode, http_endpoint,
};
use crate::collectors::elasticsearch::value_objects::{
    HttpEndpoint, NetworkNamespace, Transport, Unread,
};

/// Reads everything this box and this node will say about `resident`.
pub fn read_node(proc: &Path, resident: &ResidentNode, client: &HttpClient) -> Node {
    let mut node = Node {
        process_id: resident.process_id(),
        config_directory: resident
            .config()
            .map(|config| config.to_string_lossy().into_owned()),
        network_namespace: None,
        http: None,
        identity: None,
        cluster_settings: None,
        index_templates: None,
        component_templates: None,
        indices: None,
        error: None,
    };

    if let Err(unread) = read_into(&mut node, proc, resident, client) {
        node.error = Some(unread);
    }

    node
}

fn read_into(
    node: &mut Node,
    proc: &Path,
    resident: &ResidentNode,
    client: &HttpClient,
) -> Result<(), Unread> {
    let settings = NodeSettings::read_in(proc, resident)?;
    if settings.transport() == Transport::TlsRequired {
        return Err(Unread::new(
            "the node's settings put its HTTP listener behind TLS \
             (xpack.security.http.ssl.enabled), and v1 speaks plain HTTP only",
        ));
    }

    let namespace = NodeNamespace::of_in(proc, resident.process_id())?;
    node.network_namespace = Some(match namespace.is_ours() {
        true => NetworkNamespace::Host,
        false => NetworkNamespace::Separate,
    });

    let listeners = NodeListener::read_in(proc, resident.process_id())?;
    let endpoint = http_endpoint(&listeners, &settings)?;
    node.http = Some(endpoint.clone());

    let answers = namespace.run(|| read_answers(client, &endpoint))??;
    node.identity = Some(answers.identity);
    node.cluster_settings = Some(answers.cluster_settings);
    node.index_templates = Some(answers.index_templates);
    node.component_templates = Some(answers.component_templates);
    node.indices = Some(answers.indices);
    Ok(())
}

/// What the node said, one request per surface.
struct Answers {
    identity: NodeIdentity,
    cluster_settings: Surface<ClusterSettings>,
    index_templates: Surface<NamedDefinitions>,
    component_templates: Surface<NamedDefinitions>,
    indices: Surface<Indices>,
}

/// Every read of the node, in one pass inside its namespace.
///
/// `GET /` first and alone decisive: a node that will not say who it is has refused the read,
/// and nothing after it is asked. Every later surface fails on its own.
fn read_answers(client: &HttpClient, endpoint: &HttpEndpoint) -> Result<Answers, Unread> {
    Ok(Answers {
        identity: read_identity(client, endpoint)?,
        cluster_settings: read_cluster_settings(client, endpoint),
        index_templates: read_index_templates(client, endpoint),
        component_templates: read_component_templates(client, endpoint),
        indices: read_indices(client, endpoint),
    })
}
