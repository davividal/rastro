//! `docker network inspect`: docker's spelling of one network.

use std::collections::BTreeMap;

use serde::Deserialize;

use rastro_collector::{CollectionError, NonEmptyText};

use crate::collectors::containers::model::{AddressPool, DockerNetwork, NetworkAddressing};
use crate::collectors::containers::value_objects::{
    EngineInstant, LabelName, NetworkId, NetworkName,
};
use crate::collectors::inet::IpAddress;

/// A network as docker describes it, kept apart from rastro's meaning.
///
/// **`Containers` is deliberately not declared.** docker lists every container attached to
/// the network, and each of those containers already reports this network from its own end
/// with more detail. serde ignores what is not asked for, so not asking is how the edge
/// stays in the document exactly once.
#[derive(Debug, Clone, Deserialize)]
pub struct DockerNetworkDocument {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Id")]
    id: String,
    #[serde(rename = "Created")]
    created: String,
    #[serde(rename = "Driver")]
    driver: String,
    #[serde(rename = "Scope")]
    scope: String,
    #[serde(rename = "EnableIPv6", default)]
    ipv6_enabled: bool,
    #[serde(rename = "Internal", default)]
    internal: bool,
    #[serde(rename = "Attachable", default)]
    attachable: bool,
    #[serde(rename = "Ingress", default)]
    ingress: bool,
    #[serde(rename = "IPAM")]
    addressing: AddressingHalf,
    /// Null on a network with none, which `default` covers either way.
    #[serde(rename = "Options", default)]
    options: Option<BTreeMap<String, String>>,
    #[serde(rename = "Labels", default)]
    labels: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Clone, Deserialize)]
struct AddressingHalf {
    #[serde(rename = "Driver")]
    driver: String,
    #[serde(rename = "Config", default)]
    configured: Option<Vec<PoolEntry>>,
    #[serde(rename = "Options", default)]
    options: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Clone, Deserialize)]
struct PoolEntry {
    #[serde(rename = "Subnet")]
    subnet: String,
    /// Empty on one read of a network and filled in on a later one, measured on the same
    /// docker either side of a daemon restart, so absent means unreported rather than none.
    #[serde(rename = "Gateway", default)]
    gateway: String,
    #[serde(rename = "IPRange", default)]
    allocation_range: String,
}

impl DockerNetworkDocument {
    /// Translates docker's document into rastro's model, keyed by the name it sits under.
    pub fn to_network(&self) -> Result<(NetworkName, DockerNetwork), CollectionError> {
        let mut configured = Vec::new();
        for pool in self.addressing.configured.iter().flatten() {
            configured.push(AddressPool {
                subnet: NonEmptyText::new(pool.subnet.clone(), "subnet")?,
                gateway: IpAddress::new(pool.gateway.clone()).ok(),
                allocation_range: NonEmptyText::new(
                    pool.allocation_range.clone(),
                    "allocation range",
                )
                .ok(),
            });
        }

        let network = DockerNetwork {
            id: NetworkId::new(self.id.clone())?,
            created: EngineInstant::new(self.created.clone())?,
            driver: NonEmptyText::new(self.driver.clone(), "network driver")?,
            scope: NonEmptyText::new(self.scope.clone(), "network scope")?,
            ipv6_enabled: self.ipv6_enabled,
            internal: self.internal,
            attachable: self.attachable,
            ingress: self.ingress,
            addressing: NetworkAddressing {
                driver: NonEmptyText::new(self.addressing.driver.clone(), "ipam driver")?,
                configured,
                options: named(self.addressing.options.as_ref(), "ipam option")?,
            },
            options: named(self.options.as_ref(), "network driver option")?,
            labels: labelled(self.labels.as_ref())?,
        };

        Ok((NetworkName::new(self.name.clone())?, network))
    }
}

/// One of docker's option maps as rastro's.
fn named(
    reported: Option<&BTreeMap<String, String>>,
    kind: &str,
) -> Result<BTreeMap<NonEmptyText, String>, CollectionError> {
    let mut named = BTreeMap::new();

    for (name, value) in reported.into_iter().flatten() {
        named.insert(NonEmptyText::new(name.clone(), kind)?, value.clone());
    }

    Ok(named)
}

fn labelled(
    reported: Option<&BTreeMap<String, String>>,
) -> Result<BTreeMap<LabelName, String>, CollectionError> {
    let mut labels = BTreeMap::new();

    for (name, value) in reported.into_iter().flatten() {
        labels.insert(LabelName::new(name.clone())?, value.clone());
    }

    Ok(labels)
}
