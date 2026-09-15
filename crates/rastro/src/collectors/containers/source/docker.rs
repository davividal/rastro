//! Asking docker what it is, through its own client.

use std::fs;
use std::path::Path;

use rastro_collector::{AbsolutePath, CollectionError, WalkedTree};

use super::docker_container_document::DockerContainerDocument;
use super::docker_image_document::DockerImageDocument;
use super::docker_info::DockerInfoDocument;
use super::docker_network_document::DockerNetworkDocument;
use super::docker_version::DockerVersionDocument;
use super::docker_volume_document::DockerVolumeDocument;
use crate::collectors::canonical_tool::CanonicalTool;
use crate::collectors::containers::model::{
    DockerContainer, DockerContainers, DockerEngine, DockerImage, DockerImages, DockerNetwork,
    DockerNetworks, DockerVolume, DockerVolumes, UnreadableObject,
};
use crate::collectors::containers::value_objects::{
    ContainerId, ContainerName, ImageDigest, NetworkId, NetworkName, VolumeName,
};

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

/// Every image's id, the dangling ones included, untruncated.
///
/// **`--all`, because a dangling image is state.** It holds disk, it is usually what a
/// rebuild left behind, and `<none>:<none>` in `docker images` is the only place an
/// operator ever meets it.
const LIST_IMAGES: [&str; 4] = ["image", "ls", "--all", "--no-trunc"];

/// The flag that reduces the image list to ids alone.
const QUIET: &str = "--quiet";

/// One image.
const INSPECT_IMAGE: [&str; 2] = ["image", "inspect"];

/// Every volume's name, the anonymous ones included.
const LIST_VOLUMES: [&str; 2] = ["volume", "ls"];

/// One volume.
const INSPECT_VOLUME: [&str; 2] = ["volume", "inspect"];

/// Every network's id, the engine's own three included, untruncated.
const LIST_NETWORKS: [&str; 3] = ["network", "ls", "--no-trunc"];

/// One network.
const INSPECT_NETWORK: [&str; 2] = ["network", "inspect"];

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
/// The one tree under the engine's root that is not the engine's own bookkeeping.
///
/// Named rather than resolved, and it is the only name in this read. It is the inverse of a
/// guess: everything else under the root is sealed *because* it was listed, and this is the
/// one directory that must survive that. If a future docker renames it the effect is a loss
/// of detail the effective table still declares, not a wrong reading.
const OPERATOR_DATA: &str = "volumes";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Docker {
    tool: CanonicalTool,
    /// Where the daemon said it keeps everything, resolved once when this was constructed.
    ///
    /// **Held rather than read again, because the claim cannot wait for the facet.** The
    /// walk's table is built before any collector runs, so the trees this engine owns have
    /// to be known before [`Self::read`] has said anything. Resolving it at construction is
    /// also what the postgresql collector does with its cluster list, and for the same
    /// reason: what was found is the very thing that will be claimed.
    root_directory: Option<AbsolutePath>,
}

impl Docker {
    /// Locates the client, or reports that this host has no docker.
    pub fn detect() -> Option<Self> {
        CanonicalTool::located(PROGRAM).map(Self::using)
    }

    /// The same source over a tool the caller located, which is what the tests hand it.
    ///
    /// Resolves the root directory here rather than in [`Self::detect`], so a test drives
    /// exactly the code a real run does.
    pub fn using(tool: CanonicalTool) -> Self {
        let root_directory = resolve_root(&tool);

        Self {
            tool,
            root_directory,
        }
    }

    /// The trees this engine keeps to itself, which the walk should stop at.
    ///
    /// **Every child of the root except the operator's data**, listed rather than named.
    /// The layer store is `overlay2` under one driver, `vfs` under another and `rootfs` on
    /// docker 29 whose driver is called `overlayfs`, so neither a fixed list nor a mapping
    /// from the driver name covers every engine. Listing the root covers all of them, and
    /// covers a directory a later docker adds without rastro being told about it.
    ///
    /// Sealed rather than merely unhashed, on the same reasoning as a database's data
    /// directory: on a real box this is most of the entries — 294,525 of one machine's
    /// 376,948 under the container store were layer entries — every attribute that survives
    /// moves on the next pull, and what is actually in there is reported properly by this
    /// facet, the images by digest and the containers by name.
    ///
    /// Empty when the root is unknown, since a claim rastro cannot resolve is better left
    /// unmade.
    pub fn private_trees(&self) -> Vec<WalkedTree> {
        let Some(root) = &self.root_directory else {
            return Vec::new();
        };

        let Ok(children) = fs::read_dir(Path::new(root.as_str())) else {
            return Vec::new();
        };

        let mut trees = Vec::new();
        for child in children.flatten() {
            if !child.path().is_dir() || child.file_name() == OPERATOR_DATA {
                continue;
            }

            if let Some(tree) = child
                .path()
                .to_str()
                .and_then(|path| WalkedTree::new(path).ok())
            {
                trees.push(tree);
            }
        }

        trees.sort();
        trees
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
        let images = self.images()?;
        let volumes = self.volumes()?;
        let networks = self.networks()?;

        Ok(DockerEngine::answering(
            versions.client,
            reported.to_server(
                server.version,
                server.components,
                containers,
                images,
                volumes,
                networks,
            )?,
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

    /// The networks, read one at a time, and the ones that could not be read.
    ///
    /// **Listed by id and read by id**, unlike the volumes: two networks may not share a
    /// name, but the list prints ids untruncated and the id is what unambiguously names one
    /// to `inspect`, which also accepts a name and would then have to guess.
    fn networks(&self) -> Result<DockerNetworks, CollectionError> {
        let mut read: Vec<(NetworkName, DockerNetwork)> = Vec::new();
        let mut unreadable: Vec<UnreadableObject> = Vec::new();

        let mut listed = LIST_NETWORKS.to_vec();
        listed.push(QUIET);

        for line in self.tool.run(&listed)?.lines() {
            let id = line.trim();
            if id.is_empty() {
                continue;
            }

            let id = NetworkId::new(id)?;
            match self.inspect_network(&id) {
                Ok(network) => read.push(network),
                Err(failure) => {
                    unreadable.push(UnreadableObject::new(id.as_str(), &failure.to_string())?)
                }
            }
        }

        DockerNetworks::new(read, unreadable)
    }

    /// One network as docker describes it.
    fn inspect_network(
        &self,
        id: &NetworkId,
    ) -> Result<(NetworkName, DockerNetwork), CollectionError> {
        let mut arguments = INSPECT_NETWORK.to_vec();
        arguments.push(id.as_str());

        let documents =
            decode::<Vec<DockerNetworkDocument>>(&self.tool.run(&arguments)?, "network inspect")?;

        documents
            .first()
            .ok_or_else(|| {
                CollectionError::new(format!(
                    "`{PROGRAM} network inspect` described no network for the id {:?} it had \
                     just listed",
                    id.as_str()
                ))
            })?
            .to_network()
    }

    /// The volumes, read one at a time, and the ones that could not be read.
    fn volumes(&self) -> Result<DockerVolumes, CollectionError> {
        let mut read: Vec<(VolumeName, DockerVolume)> = Vec::new();
        let mut unreadable: Vec<UnreadableObject> = Vec::new();

        let mut listed = LIST_VOLUMES.to_vec();
        listed.push(QUIET);

        for line in self.tool.run(&listed)?.lines() {
            let name = line.trim();
            if name.is_empty() {
                continue;
            }

            let name = VolumeName::new(name)?;
            match self.inspect_volume(&name) {
                Ok(volume) => read.push(volume),
                Err(failure) => {
                    unreadable.push(UnreadableObject::new(name.as_str(), &failure.to_string())?)
                }
            }
        }

        DockerVolumes::new(read, unreadable)
    }

    /// One volume as docker describes it.
    fn inspect_volume(
        &self,
        name: &VolumeName,
    ) -> Result<(VolumeName, DockerVolume), CollectionError> {
        let mut arguments = INSPECT_VOLUME.to_vec();
        arguments.push(name.as_str());

        let documents =
            decode::<Vec<DockerVolumeDocument>>(&self.tool.run(&arguments)?, "volume inspect")?;

        documents
            .first()
            .ok_or_else(|| {
                CollectionError::new(format!(
                    "`{PROGRAM} volume inspect` described no volume for the name {:?} it had \
                     just listed",
                    name.as_str()
                ))
            })?
            .to_volume()
    }

    /// The images, read one at a time, and the ones that could not be read.
    ///
    /// One read per image for the reason the containers have: `docker build` and
    /// `docker image prune` do to an image list exactly what a cron `--rm` does to a
    /// container list, and a batched inspect would lose every image on the box to one that
    /// went away mid-run.
    fn images(&self) -> Result<DockerImages, CollectionError> {
        let mut read: Vec<(ImageDigest, DockerImage)> = Vec::new();
        let mut unreadable: Vec<UnreadableObject> = Vec::new();

        let mut listed = LIST_IMAGES.to_vec();
        listed.push(QUIET);

        for line in self.tool.run(&listed)?.lines() {
            let id = line.trim();
            if id.is_empty() {
                continue;
            }

            let id = ImageDigest::new(id)?;
            match self.inspect_image(&id) {
                Ok(image) => read.push(image),
                Err(failure) => {
                    unreadable.push(UnreadableObject::new(id.as_str(), &failure.to_string())?)
                }
            }
        }

        DockerImages::new(read, unreadable)
    }

    /// One image as docker describes it.
    fn inspect_image(
        &self,
        id: &ImageDigest,
    ) -> Result<(ImageDigest, DockerImage), CollectionError> {
        let mut arguments = INSPECT_IMAGE.to_vec();
        arguments.push(id.as_str());

        let documents =
            decode::<Vec<DockerImageDocument>>(&self.tool.run(&arguments)?, "image inspect")?;

        documents
            .first()
            .ok_or_else(|| {
                CollectionError::new(format!(
                    "`{PROGRAM} image inspect` described no image for the id {:?} it had just \
                     listed",
                    id.as_str()
                ))
            })?
            .to_image()
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

/// Where the daemon keeps its store, or nothing if it would not say.
///
/// A failure here is not reported: this runs at construction, before the facet exists to
/// carry an error, and every reason it can fail — no daemon, no permission — is a reason
/// [`Docker::read`] will fail loudly a moment later with the same message.
fn resolve_root(tool: &CanonicalTool) -> Option<AbsolutePath> {
    let reported = decode::<DockerInfoDocument>(&tool.run(&INFO).ok()?, "info").ok()?;

    reported.root_directory()
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
