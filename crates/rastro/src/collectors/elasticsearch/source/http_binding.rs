//! Which of a node's listeners serves HTTP, inferred from how the node binds, never by asking.
//!
//! **Measured on 7.17.24 and 8.15.3:** a node binds its transport port first and HTTP second,
//! each the lowest free port in its range. The default ranges, `9300-9400` for transport and
//! `9200-9300` for HTTP, overlap at 9300, so on a default node both of its ports are in the HTTP
//! range and "the listener in the HTTP range" is not an answer. Asking each listener is not an
//! option either: HTTP sent to the transport port is an error the node logs, which is a write to
//! the box. So the transport port is taken out first, as the lowest listener in its own range,
//! and exactly one listener must be left in the HTTP range. Two left is a refusal, not a pick.

use std::collections::BTreeSet;
use std::ops::RangeInclusive;

use crate::collectors::elasticsearch::source::{NodeListener, NodeSettings};
use crate::collectors::elasticsearch::value_objects::{HttpEndpoint, Unread};
use crate::collectors::inet::InetHost;

const HTTP_PORT: &str = "http.port";
const TRANSPORT_PORT: &str = "transport.port";
const DEFAULT_HTTP_PORTS: RangeInclusive<u16> = 9200..=9300;
const DEFAULT_TRANSPORT_PORTS: RangeInclusive<u16> = 9300..=9400;

/// The listener rastro may send HTTP to, and the address to dial it on.
pub fn http_endpoint(
    listeners: &[NodeListener],
    settings: &NodeSettings,
) -> Result<HttpEndpoint, Unread> {
    let http_ports = port_range(settings, HTTP_PORT, DEFAULT_HTTP_PORTS)?;
    let transport_ports = port_range(settings, TRANSPORT_PORT, DEFAULT_TRANSPORT_PORTS)?;

    let ports: BTreeSet<u16> = listeners
        .iter()
        .map(|listener| listener.port.as_u16())
        .collect();
    let transport = ports
        .iter()
        .copied()
        .find(|port| transport_ports.contains(port));

    let candidates: Vec<u16> = ports
        .iter()
        .copied()
        .filter(|port| Some(*port) != transport && http_ports.contains(port))
        .collect();

    let port = match candidates.as_slice() {
        [port] => *port,
        [] => {
            return Err(Unread::new(format!(
                "none of the node's listeners ({}) is in its HTTP range {}-{} once its transport \
                 port is set aside",
                spelled(&ports),
                http_ports.start(),
                http_ports.end()
            )));
        }
        several => {
            return Err(Unread::new(format!(
                "ports {} are all in the node's HTTP range {}-{}, so which one serves HTTP \
                 cannot be told without asking, and rastro does not pick one",
                spelled(several),
                http_ports.start(),
                http_ports.end()
            )));
        }
    };

    let bound: Vec<&NodeListener> = listeners
        .iter()
        .filter(|listener| listener.port.as_u16() == port)
        .collect();
    let host = dialled_host(&bound)?;
    let chosen = bound
        .first()
        .expect("a candidate port is one some listener holds");

    Ok(HttpEndpoint::new(host, chosen.port))
}

/// The address to dial among those the port is bound on, loopback first.
///
/// A wildcard is dialled on its own family's loopback: `::` accepts `::1` whether or not it is
/// dual-stack, and `/proc` does not publish which it is. An address that is neither loopback nor
/// a wildcard is dialled as bound, because a node given one interface listens on nothing else.
fn dialled_host(bound: &[&NodeListener]) -> Result<InetHost, Unread> {
    let spelled: Vec<&str> = bound
        .iter()
        .map(|listener| listener.host.as_str())
        .collect();

    let chosen = if spelled
        .iter()
        .any(|host| matches!(*host, "127.0.0.1" | "0.0.0.0"))
    {
        "127.0.0.1"
    } else if spelled.iter().any(|host| matches!(*host, "::1" | "::")) {
        "::1"
    } else if let Some(ipv4) = spelled.iter().find(|host| !host.contains(':')) {
        ipv4
    } else {
        spelled
            .first()
            .ok_or_else(|| Unread::new("the HTTP port is bound on no address"))?
    };

    InetHost::new(chosen).map_err(|error| Unread::new(error.to_string()))
}

/// A port setting's range, where the setting is either one port or `low-high`.
fn port_range(
    settings: &NodeSettings,
    name: &str,
    default: RangeInclusive<u16>,
) -> Result<RangeInclusive<u16>, Unread> {
    let Some(value) = settings.get(name) else {
        return Ok(default);
    };

    let unreadable = || {
        Unread::new(format!(
            "{name} is `{value}`, which is not a port or a range"
        ))
    };
    let port = |text: &str| text.trim().parse::<u16>().map_err(|_| unreadable());

    match value.split_once('-') {
        Some((low, high)) => Ok(port(low)?..=port(high)?),
        None => {
            let single = port(value)?;
            Ok(single..=single)
        }
    }
}

fn spelled<'ports>(ports: impl IntoIterator<Item = &'ports u16>) -> String {
    ports
        .into_iter()
        .map(u16::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}
