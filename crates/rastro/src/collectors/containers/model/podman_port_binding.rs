//! Where a podman container's port is published.

use rastro_collector::Observation;

use crate::collectors::inet::{InetHost, PortNumber};

/// One published binding, with the run of ports it covers.
///
/// **podman's own shape rather than docker's**, and the difference is the range: podman
/// reports `-p 8000-8010:8000-8010` as one binding covering eleven ports, where docker
/// reports eleven bindings. Flattening podman's into docker's would mean rastro inventing
/// ten entries the engine never said; keeping the engine's own form means the two dialects
/// read differently, which they do.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PodmanPortBinding {
    pub host_address: InetHost,
    pub host_port: PortNumber,
    /// How many consecutive ports this binding covers, which is `1` for the ordinary case.
    pub range: i64,
}

impl From<&PodmanPortBinding> for Observation {
    fn from(binding: &PodmanPortBinding) -> Self {
        Observation::object([
            ("host_address", Observation::from(&binding.host_address)),
            (
                "host_port",
                Observation::integer(i64::from(binding.host_port.as_u16())),
            ),
            ("range", Observation::integer(binding.range)),
        ])
    }
}
