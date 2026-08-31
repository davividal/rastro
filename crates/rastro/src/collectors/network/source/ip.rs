//! The `ip` interface.

use rastro_collector::CollectionError;

use super::ip_addr::InterfaceObject;
use super::ip_route::RouteObject;
use crate::collectors::canonical_tool::CanonicalTool;
use crate::collectors::network::model::NetworkState;

const PROGRAM: &str = "ip";

/// Ask for JSON, so the shape is rastro's rather than `ip`'s to format.
///
/// The same call the units collector makes about `systemctl`, and here the text form is
/// worse still: `ip addr show` is a multi-line stanza per interface with continuation
/// lines, and parsing it means reassembling records rather than splitting columns.
const JSON: &str = "-j";

/// Both families are asked for by name rather than taking the default.
///
/// `ip route show` with no family shows IPv4 only, silently. A fingerprint that reported
/// the IPv4 table as the routing table would be exactly the half-understood answer this
/// project refuses, and the omission would be invisible on a box that works.
const IPV4: &str = "-4";
const IPV6: &str = "-6";

/// Ask for details, because `ip` hides a route's protocol and scope when they are the default.
///
/// `print_route` in iproute2 prints `protocol` only when the kernel's `rtm_protocol` is not
/// `RTPROT_BOOT`, and `scope` only when `rtm_scope` is not `RT_SCOPE_UNIVERSE`, unless details
/// are on. `RTPROT_BOOT` is what an `ip route add` that named no protocol leaves behind, which
/// is every static route ifupdown installs, so on an ordinary Debian box the default route has
/// no protocol to read and the whole facet failed on it.
///
/// Asked for rather than inferred. Absence does mean `boot` for the way rastro invokes `ip`,
/// but that reasoning holds only while iproute2's print policy and rastro's own arguments both
/// stay as they are, and neither is rastro's to promise. This is the decision already recorded
/// for `-j` over parsing tables: prefer the source whose shape rastro chooses over the one it
/// has to infer.
const DETAILS: &str = "-d";

/// The host's networking, as a source rastro can read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ip {
    tool: CanonicalTool,
}

impl Ip {
    /// Finds `ip`, or reports that this host does not have it.
    pub fn detect() -> Option<Self> {
        CanonicalTool::located(PROGRAM).map(Self::using)
    }

    /// The same over a tool the caller located.
    pub fn using(tool: CanonicalTool) -> Self {
        Self { tool }
    }

    pub fn tool(&self) -> &CanonicalTool {
        &self.tool
    }

    /// Asks for the interfaces and both routing tables.
    ///
    /// Three runs rather than one. `ip -j addr show` answers the interface half completely,
    /// including every field `ip link` would add, so there is no fourth.
    pub fn read(&self) -> Result<NetworkState, CollectionError> {
        let interfaces = self.tool.run(&[JSON, "addr", "show"])?;
        let ipv4 = self.tool.run(&[IPV4, JSON, DETAILS, "route", "show"])?;
        let ipv6 = self.tool.run(&[IPV6, JSON, DETAILS, "route", "show"])?;

        Self::parse(&interfaces, &ipv4, &ipv6)
    }

    /// Translates the three outputs into the model.
    ///
    /// Separate from [`Self::read`] so the whole translation is exercised from a fixture,
    /// with no `ip` to run.
    pub fn parse(
        interfaces: &str,
        ipv4_routes: &str,
        ipv6_routes: &str,
    ) -> Result<NetworkState, CollectionError> {
        let interfaces: Vec<InterfaceObject> = decode(interfaces, "addr show")?;
        let interfaces = interfaces
            .iter()
            .map(InterfaceObject::to_interface)
            .collect::<Result<Vec<_>, CollectionError>>()?;

        let mut routes = Vec::new();
        for (output, family) in [(ipv4_routes, IPV4), (ipv6_routes, IPV6)] {
            let objects: Vec<RouteObject> = decode(output, &format!("{family} route show"))?;
            for object in &objects {
                routes.push(object.to_route()?);
            }
        }

        NetworkState::new(interfaces, routes)
    }
}

/// Reads one of `ip`'s JSON arrays, naming the subcommand if it will not parse.
///
/// An empty output is an empty array rather than a failure: `ip -6 route show` prints
/// nothing at all on a box with IPv6 disabled, and that is a host without IPv6 routes
/// rather than a broken run.
fn decode<T: serde::de::DeserializeOwned>(
    output: &str,
    subcommand: &str,
) -> Result<Vec<T>, CollectionError> {
    if output.trim().is_empty() {
        return Ok(Vec::new());
    }

    serde_json::from_str(output).map_err(|error| {
        CollectionError::new(format!(
            "could not read what `{PROGRAM} {subcommand}` reported as JSON: {error}"
        ))
    })
}
