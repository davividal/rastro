//! `docker inspect --type container`: docker's spelling of one container.

use serde::Deserialize;

use std::collections::BTreeMap;

use rastro_collector::{AbsolutePath, CollectionError, NonEmptyText};

use crate::collectors::containers::model::{
    ContainerCommand, ContainerEnvironment, ContainerImage, ContainerLabels, ContainerMount,
    ContainerMounts, ContainerNetwork, ContainerNetworks, ContainerPorts, ContainerState,
    DockerContainer, PublishedBinding,
};
use crate::collectors::containers::value_objects::{
    ContainerAccount, ContainerId, ContainerName, ContainerStatus, EngineInstant, ExposedPort,
    ImageDigest, ImageReference, LabelName, MountKind, NetworkId, NetworkName, VariableName,
};
use crate::collectors::inet::{HardwareAddress, InetHost, IpAddress, PortNumber};

/// Go's zero time, which is what docker prints for a stamp that has not happened.
///
/// **Measured on docker 29.8.0**: a running container reports
/// `"FinishedAt": "0001-01-01T00:00:00Z"`, not an absent field and not an empty string.
/// Recording it as it stands would put a date in the document for something that has not
/// happened, and a diff would then show a container finishing in the year one.
const ZERO_TIME: &str = "0001-01-01T00:00:00Z";

/// A container as docker describes it, kept apart from rastro's meaning.
///
/// Only the fields this facet reads are declared. serde ignores the rest, which for a docker
/// 29 container is about ten kilobytes of `HostConfig` the later slices of this collector
/// will claim one at a time.
#[derive(Debug, Clone, Deserialize)]
pub struct DockerContainerDocument {
    #[serde(rename = "Id")]
    id: String,
    #[serde(rename = "Created")]
    created: String,
    /// docker's own spelling carries a leading slash, from the days when container links made
    /// a namespace of the name.
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "RestartCount", default)]
    restart_count: i64,
    /// The image docker resolved, as a digest over its configuration.
    #[serde(rename = "Image")]
    image: String,
    #[serde(rename = "ImageManifestDescriptor", default)]
    manifest: Option<ManifestDescriptor>,
    /// The resolved executable, after the image's entrypoint and the container's command
    /// have been folded together.
    #[serde(rename = "Path")]
    path: String,
    #[serde(rename = "Args", default)]
    arguments: Vec<String>,
    #[serde(rename = "State")]
    state: StateHalf,
    /// Volume and bind mounts. **Not tmpfs**, which docker reports nowhere near here.
    #[serde(rename = "Mounts", default)]
    mounts: Vec<MountEntry>,
    #[serde(rename = "NetworkSettings", default)]
    network_settings: NetworkSettingsHalf,
    #[serde(rename = "Config")]
    config: ConfigHalf,
    #[serde(rename = "HostConfig")]
    host_config: HostConfigHalf,
}

#[derive(Debug, Clone, Deserialize)]
struct ManifestDescriptor {
    #[serde(rename = "digest")]
    digest: String,
}

#[derive(Debug, Clone, Deserialize)]
struct StateHalf {
    #[serde(rename = "Status")]
    status: String,
    #[serde(rename = "ExitCode", default)]
    exit_code: i64,
    #[serde(rename = "Error", default)]
    error: String,
    #[serde(rename = "OOMKilled", default)]
    oom_killed: bool,
    #[serde(rename = "StartedAt", default)]
    started_at: String,
    #[serde(rename = "FinishedAt", default)]
    finished_at: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ConfigHalf {
    /// The reference as the operator gave it, which is a different fact from the resolved
    /// digest above and is why both are read.
    #[serde(rename = "Image")]
    image: String,
    /// Empty where the image decides, rather than absent.
    #[serde(rename = "User", default)]
    user: String,
    #[serde(rename = "WorkingDir", default)]
    working_directory: String,
    /// `NAME=value` entries, the image's own environment included, which is honest: it is
    /// the environment the process has.
    #[serde(rename = "Env", default)]
    environment: Vec<String>,
    /// Null on a container with none, which `default` covers either way.
    #[serde(rename = "Labels", default)]
    labels: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
struct HostConfigHalf {
    #[serde(rename = "AutoRemove", default)]
    auto_remove: bool,
    /// Destination to option string, and the only place a `--tmpfs` mount appears at all.
    /// Null on a container with none, which `default` covers either way.
    #[serde(rename = "Tmpfs", default)]
    tmpfs: BTreeMap<String, String>,
}

/// What the engine did about the ports, as opposed to what was asked of it.
#[derive(Debug, Clone, Default, Deserialize)]
struct NetworkSettingsHalf {
    /// Port to bindings, where a port the image exposes and nobody published is present
    /// with a null value rather than absent.
    #[serde(rename = "Ports", default)]
    ports: BTreeMap<String, Option<Vec<BindingEntry>>>,
    #[serde(rename = "Networks", default)]
    networks: BTreeMap<String, NetworkEntry>,
}

/// One network as docker describes the container's end of it.
///
/// docker writes `null` for what was not asked for and empty strings for what does not
/// exist, so almost everything here is optional in one of those two ways.
#[derive(Debug, Clone, Deserialize)]
struct NetworkEntry {
    /// What the container asked IPAM for, absent where it asked for nothing.
    #[serde(rename = "IPAMConfig", default)]
    requested: Option<RequestedAddresses>,
    #[serde(rename = "Aliases", default)]
    aliases: Option<Vec<String>>,
    #[serde(rename = "MacAddress", default)]
    hardware_address: String,
    #[serde(rename = "NetworkID", default)]
    network_id: String,
    #[serde(rename = "IPAddress", default)]
    address: String,
    #[serde(rename = "GlobalIPv6Address", default)]
    ipv6_address: String,
}

#[derive(Debug, Clone, Deserialize)]
struct RequestedAddresses {
    #[serde(rename = "IPv4Address", default)]
    address: String,
    #[serde(rename = "IPv6Address", default)]
    ipv6_address: String,
}

#[derive(Debug, Clone, Deserialize)]
struct BindingEntry {
    #[serde(rename = "HostIp")]
    host_address: String,
    #[serde(rename = "HostPort")]
    host_port: String,
}

/// One entry of docker's own mount list.
#[derive(Debug, Clone, Deserialize)]
struct MountEntry {
    #[serde(rename = "Type")]
    kind: String,
    /// Only a volume has one.
    #[serde(rename = "Name", default)]
    name: String,
    /// A bind's host path, or the directory the engine keeps a volume in.
    #[serde(rename = "Source", default)]
    source: String,
    #[serde(rename = "Destination")]
    destination: String,
    /// Only a volume has one.
    #[serde(rename = "Driver", default)]
    driver: String,
    #[serde(rename = "RW", default)]
    writable: bool,
    #[serde(rename = "Propagation", default)]
    propagation: String,
}

impl DockerContainerDocument {
    /// Translates docker's document into rastro's model, keyed by the name it will sit under.
    pub fn to_container(&self) -> Result<(ContainerName, DockerContainer), CollectionError> {
        let container = DockerContainer {
            id: ContainerId::new(self.id.clone())?,
            created: EngineInstant::new(self.created.clone())?,
            image: ContainerImage {
                reference: ImageReference::new(self.config.image.clone())?,
                id: ImageDigest::new(self.image.clone())?,
                manifest_digest: match &self.manifest {
                    Some(manifest) => Some(ImageDigest::new(manifest.digest.clone())?),
                    None => None,
                },
            },
            command: ContainerCommand {
                path: NonEmptyText::new(self.path.clone(), "container command")?,
                arguments: self.arguments.clone(),
            },
            state: ContainerState {
                status: ContainerStatus::new(self.state.status.clone())?,
                exit_code: self.state.exit_code,
                error: NonEmptyText::new(self.state.error.clone(), "container error").ok(),
                oom_killed: self.state.oom_killed,
                started_at: instant(&self.state.started_at)?,
                finished_at: instant(&self.state.finished_at)?,
                restart_count: self.restart_count,
            },
            user: ContainerAccount::new(self.config.user.clone()).ok(),
            working_directory: AbsolutePath::new(
                self.config.working_directory.clone(),
                "container working directory",
            )
            .ok(),
            environment: self.environment()?,
            labels: self.labels()?,
            mounts: self.mounts()?,
            networks: self.networks()?,
            ports: self.ports()?,
            auto_remove: self.host_config.auto_remove,
        };

        Ok((
            ContainerName::new(self.name.trim_start_matches('/'))?,
            container,
        ))
    }
}

impl DockerContainerDocument {
    /// The environment, split on the first `=` of each entry.
    ///
    /// **The first, and only the first.** A value is free to hold as many as it likes, and
    /// `DSN=postgres://app:pw@db/app?a=b` would be corrupted by any other reading. An entry
    /// with no `=` at all is refused rather than guessed at: docker writes `NAME=value`, so
    /// its absence means this is not the list rastro thinks it is.
    fn environment(&self) -> Result<ContainerEnvironment, CollectionError> {
        let mut variables = Vec::new();

        for entry in &self.config.environment {
            let Some((name, value)) = entry.split_once('=') else {
                return Err(CollectionError::new(format!(
                    "docker reported the environment entry {entry:?}, which names no value, \
                     so the environment was misread"
                )));
            };

            variables.push((VariableName::new(name)?, value.to_owned()));
        }

        Ok(ContainerEnvironment::new(variables))
    }

    /// Both of docker's accounts of what is mounted, merged on the destination.
    ///
    /// **Two sources rather than one, because a tmpfs is in neither list the other is in.**
    /// Measured on docker 26.1.5: `--tmpfs /scratch` produces no `Mounts` entry at all and
    /// appears only as `HostConfig.Tmpfs`, so reading the mount list alone would lose every
    /// tmpfs on the box without saying so.
    fn mounts(&self) -> Result<ContainerMounts, CollectionError> {
        let mut mounts = Vec::new();

        for entry in &self.mounts {
            mounts.push((
                AbsolutePath::new(entry.destination.clone(), "mount destination")?,
                ContainerMount {
                    kind: MountKind::new(entry.kind.clone())?,
                    name: NonEmptyText::new(entry.name.clone(), "volume name").ok(),
                    source: AbsolutePath::new(entry.source.clone(), "mount source").ok(),
                    driver: NonEmptyText::new(entry.driver.clone(), "volume driver").ok(),
                    writable: entry.writable,
                    propagation: NonEmptyText::new(entry.propagation.clone(), "propagation").ok(),
                    options: None,
                },
            ));
        }

        for (destination, options) in &self.host_config.tmpfs {
            mounts.push((
                AbsolutePath::new(destination.clone(), "tmpfs destination")?,
                ContainerMount {
                    kind: MountKind::tmpfs(),
                    name: None,
                    source: None,
                    driver: None,
                    // A tmpfs is writable unless its own options say `ro`, which is where
                    // docker keeps that fact rather than in a flag of its own.
                    writable: !is_read_only(options),
                    propagation: None,
                    options: NonEmptyText::new(options.clone(), "tmpfs options").ok(),
                },
            ));
        }

        ContainerMounts::new(mounts)
    }

    /// The networks, with each end's requested addresses kept apart from its assigned ones.
    fn networks(&self) -> Result<ContainerNetworks, CollectionError> {
        let mut networks = Vec::new();

        for (name, entry) in &self.network_settings.networks {
            let mut aliases = Vec::new();
            for alias in entry.aliases.iter().flatten() {
                aliases.push(NonEmptyText::new(alias.clone(), "network alias")?);
            }
            // Sorted, because they arrive in the order they were declared, which is the
            // operator's order rather than anything the engine promises.
            aliases.sort();

            let requested = entry.requested.as_ref();
            networks.push((
                NetworkName::new(name.clone())?,
                ContainerNetwork {
                    aliases,
                    address: address(&entry.address),
                    ipv6_address: address(&entry.ipv6_address),
                    hardware_address: HardwareAddress::new(entry.hardware_address.clone()).ok(),
                    network_id: NetworkId::new(entry.network_id.clone()).ok(),
                    requested_address: requested.and_then(|asked| address(&asked.address)),
                    requested_ipv6_address: requested
                        .and_then(|asked| address(&asked.ipv6_address)),
                },
            ));
        }

        ContainerNetworks::new(networks)
    }

    /// The port table, with an unpublished port kept and given no bindings.
    fn ports(&self) -> Result<ContainerPorts, CollectionError> {
        let mut ports = Vec::new();

        for (key, bindings) in &self.network_settings.ports {
            let mut published = Vec::new();

            for binding in bindings.iter().flatten() {
                published.push(PublishedBinding {
                    host_address: InetHost::new(binding.host_address.clone())?,
                    host_port: PortNumber::parse(&binding.host_port)?,
                });
            }

            ports.push((ExposedPort::parse(key)?, published));
        }

        ContainerPorts::new(ports)
    }

    fn labels(&self) -> Result<ContainerLabels, CollectionError> {
        let mut labels = Vec::new();

        for (name, value) in &self.config.labels {
            labels.push((LabelName::new(name.clone())?, value.clone()));
        }

        Ok(ContainerLabels::new(labels))
    }
}

/// An address docker filled in, or absent for the empty string it writes when there is none.
fn address(reported: &str) -> Option<IpAddress> {
    IpAddress::new(reported).ok()
}

/// Whether a tmpfs option string asks for a read-only mount.
///
/// Split on commas, which is safe here and would not be for a bind: these are tmpfs mount
/// options, where no value holds a comma, and only whole-token equality is asked.
fn is_read_only(options: &str) -> bool {
    options.split(',').any(|option| option == "ro")
}

/// A stamp docker filled in, or absent for one that has not happened.
fn instant(reported: &str) -> Result<Option<EngineInstant>, CollectionError> {
    if reported.is_empty() || reported == ZERO_TIME {
        return Ok(None);
    }

    Ok(Some(EngineInstant::new(reported)?))
}
