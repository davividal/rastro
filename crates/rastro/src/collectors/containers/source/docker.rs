//! Asking docker what it is, through its own client.

use rastro_collector::CollectionError;

use super::docker_container_document::DockerContainerDocument;
use super::docker_info::DockerInfoDocument;
use super::docker_version::DockerVersionDocument;
use crate::collectors::canonical_tool::CanonicalTool;
use crate::collectors::containers::model::{
    DockerContainer, DockerContainers, DockerEngine, UnreadableObject,
};
use crate::collectors::containers::value_objects::{ContainerId, ContainerName};

/// docker's client, which is the only interface the engine documents as stable.
const PROGRAM: &str = "docker";

/// The probe that establishes whether a daemon is answering, and what both halves are.
const VERSION: [&str; 3] = ["version", "--format", "{{json .}}"];

/// What the answering daemon runs with.
const INFO: [&str; 3] = ["info", "--format", "{{json .}}"];

/// Every container's id, stopped ones included, untruncated.
///
/// **`--all`, because a container that has exited is exactly what a fingerprint is taken to
/// find.** `docker ps` alone hides it, and "the box that used to run this" is the state an
/// operator is looking for when they diff.
const LIST: [&str; 4] = ["ps", "--all", "--no-trunc", "--quiet"];

/// One container, pinned to a container.
///
/// `--type container` because `docker inspect` will otherwise answer about an image or a
/// volume of the same name, and a document of the wrong kind would fail to parse in a way
/// that reads like a broken container.
const INSPECT: [&str; 3] = ["inspect", "--type", "container"];

/// A docker client found on this host, ready to be asked.
///
/// **The CLI rather than the socket, and reading rather than parsing.** docker's client is a
/// formatter over the daemon's API, so `--format '{{json .}}'` yields the API's own JSON: a
/// format rastro chooses rather than a table whose columns it would have to guess. Talking to
/// `/var/run/docker.sock` directly would buy the same document for the price of an HTTP client
/// and a version negotiation.
///
/// Both reads pass the gate a fingerprint tool has to pass, and it was measured on a quiet box
/// running docker 29.8.0: a full stat inventory of `/var/lib/docker`, `/var/lib/containerd`,
/// `/run/docker` and root's home was unchanged across `version`, `info`, `ps`, `inspect` and
/// the volume and network reads, against a control interval that proved the box was otherwise
/// still. Unlike nginx, docker reports its effective state without touching the host, so
/// there is no configuration here for rastro to parse.
///
/// A last property comes free from the execution seam: it clears the environment, so no
/// `DOCKER_HOST` and no client context can point this at a daemon on another box. The facet is
/// about the box rastro is running on, structurally.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Docker {
    tool: CanonicalTool,
}

impl Docker {
    /// Locates the client, or reports that this host has no docker.
    pub fn detect() -> Option<Self> {
        CanonicalTool::located(PROGRAM).map(Self::using)
    }

    /// The same source over a tool the caller located, which is what the tests hand it.
    pub fn using(tool: CanonicalTool) -> Self {
        Self { tool }
    }

    /// docker as this box has it: the client, and the daemon if one answered.
    pub fn read(&self) -> Result<DockerEngine, CollectionError> {
        // Both streams, because this is one of the tools that answers on the wrong one: the
        // connection failure lands on stderr while the exit status stays zero.
        let probed = self.tool.run_capturing_stderr(&VERSION)?;
        let versions = decode::<DockerVersionDocument>(&probed.stdout, "version")?.to_versions()?;

        let Some(server) = versions.server else {
            return Ok(DockerEngine::unreachable(versions.client, &probed.stderr));
        };

        let reported = decode::<DockerInfoDocument>(&self.tool.run(&INFO)?, "info")?;
        let containers = self.containers()?;

        Ok(DockerEngine::answering(
            versions.client,
            reported.to_server(server.version, server.components, containers)?,
        ))
    }

    /// The containers, read one at a time, and the ones that could not be read.
    ///
    /// **One read per container rather than one read for all of them, and the reason is the
    /// race.** `docker inspect` given several ids exits non-zero if any one of them has gone,
    /// and the execution seam refuses a non-zero exit's output entirely, so a single
    /// `--rm` container ending mid-run would cost the whole facet every other container on
    /// the box. Read one at a time, that loss is one entry in `unreadable`, named and
    /// recorded.
    ///
    /// The cost is one subprocess per container, which is what the concurrency the collectors
    /// run under is for.
    fn containers(&self) -> Result<DockerContainers, CollectionError> {
        let mut read: Vec<(ContainerName, DockerContainer)> = Vec::new();
        let mut unreadable: Vec<UnreadableObject> = Vec::new();

        for line in self.tool.run(&LIST)?.lines() {
            let listed = line.trim();
            if listed.is_empty() {
                continue;
            }

            let id = ContainerId::new(listed)?;
            match self.inspect(&id) {
                Ok(container) => read.push(container),
                Err(failure) => {
                    unreadable.push(UnreadableObject::new(id.as_str(), &failure.to_string())?)
                }
            }
        }

        DockerContainers::new(read, unreadable)
    }

    /// One container as docker describes it.
    fn inspect(
        &self,
        id: &ContainerId,
    ) -> Result<(ContainerName, DockerContainer), CollectionError> {
        let mut arguments = INSPECT.to_vec();
        arguments.push(id.as_str());

        let documents =
            decode::<Vec<DockerContainerDocument>>(&self.tool.run(&arguments)?, "inspect")?;

        documents
            .first()
            .ok_or_else(|| {
                CollectionError::new(format!(
                    "`{PROGRAM} inspect` described no container for the id {:?} it had just                      listed",
                    id.as_str()
                ))
            })?
            .to_container()
    }
}

/// Reads one of docker's JSON documents, naming the subcommand if it will not parse.
///
/// The message names the subcommand rather than quoting the output, which for `info` is ten
/// kilobytes and would bury the reason it failed.
fn decode<T: serde::de::DeserializeOwned>(
    output: &str,
    subcommand: &str,
) -> Result<T, CollectionError> {
    serde_json::from_str(output).map_err(|error| {
        CollectionError::new(format!(
            "could not read what `{PROGRAM} {subcommand}` reported as JSON: {error}"
        ))
    })
}
