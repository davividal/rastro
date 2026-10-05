//! One node, read in the only order that asks nothing blind.
//!
//! Settings, then whether they allow plain HTTP, then the namespace, then the listeners, then
//! which of them serves HTTP, and only then a request, made inside the node's namespace. Each
//! step's refusal stops the read there, and what the earlier steps established is kept.

use std::path::Path;

use crate::collectors::elasticsearch::model::{
    ClusterSettings, IlmPolicies, Indices, NamedDefinitions, Node, NodeIdentity, NodeLocal,
    Plugins, SnapshotRepositories, Surface,
};
use crate::collectors::elasticsearch::source::blocks_answer::has_no_master;
use crate::collectors::elasticsearch::source::cluster_settings_answer::read_cluster_settings;
use crate::collectors::elasticsearch::source::indices_answer::read_indices;
use crate::collectors::elasticsearch::source::lifecycle_answer::{
    read_ilm_policies, read_ingest_pipelines, read_snapshot_repositories,
};
use crate::collectors::elasticsearch::source::node_local_answer::read_node_local;
use crate::collectors::elasticsearch::source::plugins_answer::read_plugins;
use crate::collectors::elasticsearch::source::root_answer::read_identity;
use crate::collectors::elasticsearch::source::templates_answer::{
    read_component_templates, read_index_templates,
};
use crate::collectors::elasticsearch::source::{
    HttpClient, NodeListener, NodeNamespace, NodeSettings, ProcessOwner, ResidentNode,
    http_endpoint,
};
use crate::collectors::elasticsearch::value_objects::{
    HttpEndpoint, NetworkNamespace, NodeCredential, Release, ReleaseSupport, Unread,
};

/// Reads everything this box and this node will say about `resident`.
pub fn read_node(
    proc: &Path,
    resident: &ResidentNode,
    client: &HttpClient,
    credential: Option<&NodeCredential>,
) -> Node {
    let mut node = Node {
        process_id: resident.process_id(),
        config_directory: resident
            .config()
            .map(|config| config.to_string_lossy().into_owned()),
        release: resident.release().map(|release| release.to_string()),
        installed_release: resident
            .installed_release()
            .map(|release| release.to_string()),
        network_namespace: None,
        http: None,
        identity: None,
        cluster_settings: None,
        index_templates: None,
        component_templates: None,
        indices: None,
        ilm_policies: None,
        ingest_pipelines: None,
        snapshot_repositories: None,
        plugins: None,
        node_local: None,
        unsupported: None,
        not_read: None,
        error: None,
    };

    if let Some(file) = resident.refused() {
        node.not_read = Some(Unread::not_read(format!(
            "rastro may not read /proc/{}/{file}, which takes root or the node's own account",
            resident.process_id()
        )));
        return node;
    }
    let Some(release) = resident.release() else {
        node.error = Some(Unread::new(
            "the node's release could not be read: its install's lib/ holds no single \
             elasticsearch-<version>.jar rastro could list",
        ));
        return node;
    };
    match release.support() {
        ReleaseSupport::Supported(_) => {}
        ReleaseSupport::ReadAs(closest) => {
            node.unsupported = Some(format!(
                "version {release} is not supported, read as {closest}"
            ));
        }
        ReleaseSupport::BelowSeven => {
            node.not_read = Some(Unread::new(format!(
                "version {release} is below 7, which rastro does not read"
            )));
            return node;
        }
    }

    let client = &for_the_account_of(resident, client, credential);
    match read_into(&mut node, proc, resident, release, client) {
        Ok(()) => {}
        Err(unread) if unread.is_not_read() => node.not_read = Some(unread),
        Err(unread) => node.error = Some(unread),
    }

    node
}

fn read_into(
    node: &mut Node,
    proc: &Path,
    resident: &ResidentNode,
    release: Release,
    client: &HttpClient,
) -> Result<(), Unread> {
    let settings = NodeSettings::read_in(proc, resident)?;

    let namespace = NodeNamespace::of_in(proc, resident.process_id())?;
    node.network_namespace = Some(match namespace.is_ours() {
        true => NetworkNamespace::Host,
        false => NetworkNamespace::Separate,
    });

    let listeners = NodeListener::read_in(proc, resident.process_id())?;
    let endpoint = http_endpoint(&listeners, &settings)?.over(settings.transport());
    node.http = Some(endpoint.clone());

    let held = HeldListener {
        proc: proc.to_path_buf(),
        process_id: resident.process_id(),
        start: resident.start(),
        port: endpoint.port().as_u16(),
    };
    let client = client
        .clone()
        .checking_the_peer_with(move || held.still_the_nodes());
    let client = &client;

    let answers = namespace.run(|| read_answers(client, &endpoint, release))??;
    // The answer is this node's only where it names the release the node's install holds.
    if answers.identity.version.number != release.to_string() {
        return Err(Unread::new(format!(
            "the node answers as version {}, and its install holds {release}, so the listener \
             asked may not be this node's or its install changed under it",
            answers.identity.version.number
        )));
    }
    node.identity = Some(answers.identity);
    node.cluster_settings = Some(answers.cluster_settings);
    node.index_templates = Some(answers.index_templates);
    node.component_templates = Some(answers.component_templates);
    node.indices = Some(answers.indices);
    node.ilm_policies = Some(answers.ilm_policies);
    node.ingest_pipelines = Some(answers.ingest_pipelines);
    node.snapshot_repositories = Some(answers.snapshot_repositories);
    node.plugins = Some(answers.plugins);
    node.node_local = Some(answers.node_local);
    Ok(())
}

/// `client`, sending the operator's credential where `resident` runs as its account, and
/// nothing otherwise.
fn for_the_account_of(
    resident: &ResidentNode,
    client: &HttpClient,
    credential: Option<&NodeCredential>,
) -> HttpClient {
    let Some(credential) = credential else {
        return client.clone();
    };
    match resident.owner().map(ProcessOwner::uid) {
        Some(uid) if uid == credential.recipient() => client
            .clone()
            .authenticating(Some(credential.credential().clone())),
        owner => client.clone().withholding(format!(
            "the credential is given only to nodes run by user id {}, and this one runs as {}",
            credential.recipient(),
            owner.map_or_else(
                || "an account that could not be read".to_owned(),
                |uid| { format!("user id {uid}") }
            )
        )),
    }
}

/// The listener a node was dialled on, which must still be the node's before each request.
struct HeldListener {
    proc: std::path::PathBuf,
    process_id: u32,

    /// The process's start as the census found it: an id alone can pass to a later process.
    start: Option<u64>,
    port: u16,
}

impl HeldListener {
    fn still_the_nodes(&self) -> Result<(), Unread> {
        let now = ResidentNode::start_of_in(&self.proc, self.process_id);
        if self.start.is_none() || now != self.start {
            return Err(Unread::new(format!(
                "process {} was restarted or replaced since it was found, so nothing more is sent \
                 to it",
                self.process_id
            )));
        }
        let listeners = NodeListener::read_in(&self.proc, self.process_id)?;
        match listeners
            .iter()
            .any(|listener| listener.port.as_u16() == self.port)
        {
            true => Ok(()),
            false => Err(Unread::new(format!(
                "the node no longer holds its listener on port {}, so nothing more is sent there",
                self.port
            ))),
        }
    }
}

/// What the node said, one request per surface.
struct Answers {
    identity: NodeIdentity,
    cluster_settings: Surface<ClusterSettings>,
    index_templates: Surface<NamedDefinitions>,
    component_templates: Surface<NamedDefinitions>,
    indices: Surface<Indices>,
    ilm_policies: Surface<IlmPolicies>,
    ingest_pipelines: Surface<NamedDefinitions>,
    snapshot_repositories: Surface<SnapshotRepositories>,
    plugins: Surface<Plugins>,
    node_local: Surface<NodeLocal>,
}

/// Every read of the node, in one pass inside its namespace.
///
/// `GET /` first and alone decisive: a node that will not say who it is has refused the read,
/// and nothing after it is asked. Every later surface fails on its own.
fn read_answers(
    client: &HttpClient,
    endpoint: &HttpEndpoint,
    release: Release,
) -> Result<Answers, Unread> {
    let identity = read_identity(client, endpoint)?;
    // Measured on cells 15 and 28: each cluster-wide read of a node with no master waits out the
    // 30 s master timeout and answers 503. A node that never formed says so in `GET /`, `_na_`;
    // one that lost its master keeps its UUID, and its blocks say so. A blocks read that fails is
    // no evidence either way, so the cluster-wide reads are then asked as before.
    let cluster_wide = identity.cluster_uuid.is_some()
        && !has_no_master(client, endpoint, release).unwrap_or(false);

    Ok(Answers {
        cluster_settings: cluster_read(cluster_wide, || read_cluster_settings(client, endpoint)),
        index_templates: cluster_read(cluster_wide, || read_index_templates(client, endpoint)),
        component_templates: cluster_read(cluster_wide, || {
            read_component_templates(client, endpoint)
        }),
        indices: cluster_read(cluster_wide, || read_indices(client, endpoint)),
        ilm_policies: cluster_read(cluster_wide, || read_ilm_policies(client, endpoint)),
        ingest_pipelines: cluster_read(cluster_wide, || read_ingest_pipelines(client, endpoint)),
        snapshot_repositories: cluster_read(cluster_wide, || {
            read_snapshot_repositories(client, endpoint)
        }),
        plugins: read_plugins(client, endpoint),
        node_local: read_node_local(client, endpoint),
        identity,
    })
}

/// A cluster-wide surface, read only where the node has a cluster to read it from.
fn cluster_read<Answer>(
    cluster_wide: bool,
    read: impl FnOnce() -> Surface<Answer>,
) -> Surface<Answer> {
    match cluster_wide {
        true => read(),
        false => Err(Unread::not_read(
            "the node has no master, so there is no cluster state to read",
        )),
    }
}
