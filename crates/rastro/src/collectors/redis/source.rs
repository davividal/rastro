//! How a server is found and read: one module per host interface.

mod installed_servers;
mod reply;
mod resident_servers;
mod resp_connection;
mod server_discovery;

pub use installed_servers::InstalledServers;
pub use reply::Reply;
pub use resident_servers::{ResidentServer, resident_servers};
pub use resp_connection::{RespConnection, ServerStream};
pub use server_discovery::{DialTarget, DiscoveredServer, discover};
