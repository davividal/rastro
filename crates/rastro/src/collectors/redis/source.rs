//! How a server is found and read: one module per host interface.

mod installed_servers;
mod resident_servers;

pub use installed_servers::InstalledServers;
pub use resident_servers::{ResidentServer, resident_servers};
