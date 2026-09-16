//! `docker version --format '{{json .}}'`: docker's spelling of who is speaking.

use serde::Deserialize;

use rastro_collector::CollectionError;

use crate::collectors::containers::value_objects::EngineVersion;

/// The probe's document, kept apart from rastro's meaning.
///
/// **This is the detection probe, and the choice was measured rather than guessed.** On a box
/// where docker is installed and its daemon is not answering, docker 29.8.0's `version` exits
/// *zero*, prints this document with `Server` null, and writes the connection failure to
/// stderr, while `info` and `ps` both exit 1. So this is the one read that can tell an
/// unreachable daemon apart from a broken one without a non-zero exit standing in for both.
///
/// **On an older client it cannot, and that is left alone.** Debian 13's docker, 26.1.5,
/// exits 1 for the same read, so the seam refuses the document it printed and the facet is an
/// `error` carrying docker's complaint. Reading stdout from a failed run to recover it would
/// mean giving up the rule that a non-zero exit yields nothing, which is worth more than this
/// distinction. `docs/decisions.md` has both measurements.
#[derive(Debug, Clone, Deserialize)]
pub struct DockerVersionDocument {
    #[serde(rename = "Client")]
    client: ClientHalf,
    /// Null when nothing answered on the socket.
    #[serde(rename = "Server")]
    server: Option<ServerHalf>,
}

#[derive(Debug, Clone, Deserialize)]
struct ClientHalf {
    #[serde(rename = "Version")]
    version: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ServerHalf {
    #[serde(rename = "Version")]
    version: String,
    /// The engine's own build first, then the containerd, runc and init it runs.
    #[serde(rename = "Components", default)]
    components: Vec<Component>,
}

#[derive(Debug, Clone, Deserialize)]
struct Component {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Version")]
    version: String,
}

/// What the probe established: the client, and the server if one answered.
pub struct ProbedVersions {
    pub client: EngineVersion,
    pub server: Option<ServerVersions>,
}

/// The answering daemon's version, and the versions of what it runs.
pub struct ServerVersions {
    pub version: EngineVersion,
    /// Name to version, as docker names them: `Engine`, `containerd`, `runc`, `docker-init`.
    pub components: Vec<(String, EngineVersion)>,
}

impl DockerVersionDocument {
    /// Translates docker's document into rastro's terms.
    ///
    /// A component whose version is blank is dropped rather than failing the read: it is one
    /// line of a report about a box whose engine is otherwise fully described, and docker
    /// leaves the field empty for a component that did not answer.
    pub fn to_versions(&self) -> Result<ProbedVersions, CollectionError> {
        let server = match &self.server {
            None => None,
            Some(half) => Some(ServerVersions {
                version: EngineVersion::new(half.version.clone())?,
                components: half
                    .components
                    .iter()
                    .filter_map(|component| {
                        EngineVersion::new(component.version.clone())
                            .ok()
                            .map(|version| (component.name.clone(), version))
                    })
                    .collect(),
            }),
        };

        Ok(ProbedVersions {
            client: EngineVersion::new(self.client.version.clone())?,
            server,
        })
    }
}
