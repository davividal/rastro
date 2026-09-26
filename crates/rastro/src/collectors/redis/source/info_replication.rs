//! The `INFO replication` reply.
//!
//! ```text
//! role:master
//! connected_slaves:1
//! slave0:ip=10.0.0.2,port=6379,state=online,offset=5210,lag=0
//! ```

use rastro_collector::CollectionError;

use super::info_fields::info_fields;
use crate::collectors::redis::model::Replication;

const ROLE: &str = "role";
const MASTER_HOST: &str = "master_host";
const MASTER_PORT: &str = "master_port";
const MASTER_LINK_STATUS: &str = "master_link_status";

/// The reader of `INFO replication`.
pub struct InfoReplication;

impl InfoReplication {
    /// The server's place in replication; a reply naming no role was misread.
    pub fn parse(text: &str) -> Result<Replication, CollectionError> {
        let fields = info_fields(text);

        let role = fields.get(ROLE).cloned().ok_or_else(|| {
            CollectionError::new("the server's INFO replication names no role, so it was misread")
        })?;
        let master = match (fields.get(MASTER_HOST), fields.get(MASTER_PORT)) {
            (Some(host), Some(port)) => Some(address_of(host, port)),
            _ => None,
        };

        // A connected replica's line, `slave<N>`, is the only one carrying an address, so the
        // address is what picks them out; `slave_repl_offset` and its siblings carry none.
        let replicas: Vec<String> = fields
            .values()
            .filter_map(|value| replica_address(value))
            .collect();

        Ok(Replication {
            role,
            master,
            master_link_status: fields.get(MASTER_LINK_STATUS).cloned(),
            replicas,
        })
    }
}

/// `host:port` out of a replica's `ip=…,port=…,…` line.
fn replica_address(value: &str) -> Option<String> {
    let attribute = |name: &str| {
        value
            .split(',')
            .find_map(|pair| pair.strip_prefix(name)?.strip_prefix('='))
    };

    Some(address_of(attribute("ip")?, attribute("port")?))
}

/// An address and port in the one spelling that stays unambiguous for IPv6.
fn address_of(host: &str, port: &str) -> String {
    match host.contains(':') {
        true => format!("[{host}]:{port}"),
        false => format!("{host}:{port}"),
    }
}
