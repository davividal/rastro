//! Asking docker what it is, through its own client.

use rastro_collector::CollectionError;

use super::docker_info::DockerInfoDocument;
use super::docker_version::DockerVersionDocument;
use crate::collectors::canonical_tool::CanonicalTool;
use crate::collectors::containers::model::DockerEngine;

/// docker's client, which is the only interface the engine documents as stable.
const PROGRAM: &str = "docker";

/// The probe that establishes whether a daemon is answering, and what both halves are.
const VERSION: [&str; 3] = ["version", "--format", "{{json .}}"];

/// What the answering daemon runs with.
const INFO: [&str; 3] = ["info", "--format", "{{json .}}"];

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

        Ok(DockerEngine::answering(
            versions.client,
            reported.to_server(server.version, server.components)?,
        ))
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
