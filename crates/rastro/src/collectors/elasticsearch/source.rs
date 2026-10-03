//! How a node is read: one module per host interface.

mod api_value_of;
mod cluster_settings_answer;
mod http_binding;
mod http_client;
mod in_root;
mod indices_answer;
mod java_argument_file;
mod json_answer;
mod lifecycle_answer;
mod node_listeners;
mod node_namespace;
mod node_reader;
mod node_settings;
mod plugins_answer;
mod resident_node;
mod root_answer;
mod templates_answer;

pub use http_binding::http_endpoint;
pub use http_client::HttpClient;
pub use in_root::host_directory_of;
pub use node_listeners::NodeListener;
pub use node_namespace::NodeNamespace;
pub use node_reader::read_node;
pub use node_settings::NodeSettings;
pub use resident_node::{Census, ResidentNode};
