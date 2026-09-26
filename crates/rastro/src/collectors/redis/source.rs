//! How a server is found and read: one module per host interface.

mod acl_list;
mod config_get;
mod config_password;
mod data_directory;
mod info_fields;
mod info_replication;
mod info_server;
mod installed_servers;
mod module_list;
mod reply;
mod resident_servers;
mod resp_connection;
mod server_discovery;
mod server_inventory;
mod server_password;
mod server_unit;

pub use acl_list::AclList;
pub use config_get::ConfigGet;
pub use config_password::requirepass_in;
pub use data_directory::data_directory_of;
pub use info_fields::info_fields;
pub use info_replication::InfoReplication;
pub use info_server::InfoServer;
pub use installed_servers::InstalledServers;
pub use module_list::ModuleList;
pub use reply::Reply;
pub use resident_servers::{ResidentServer, resident_servers};
pub use resp_connection::{RespConnection, ServerStream};
pub use server_discovery::{DialTarget, DiscoveredServer, discover};
pub use server_inventory::read_installation;
pub use server_password::{Credential, password_for};
pub use server_unit::{ServerStart, start_of, unit_of};
