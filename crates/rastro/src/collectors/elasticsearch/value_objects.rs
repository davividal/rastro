//! The collector's own small types.

mod http_endpoint;
mod network_namespace;
mod transport;
mod unread;

pub use http_endpoint::HttpEndpoint;
pub use network_namespace::NetworkNamespace;
pub use transport::Transport;
pub use unread::Unread;
