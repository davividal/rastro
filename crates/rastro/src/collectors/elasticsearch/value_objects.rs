//! The collector's own small types.

mod http_endpoint;
mod transport;
mod unread;

pub use http_endpoint::HttpEndpoint;
pub use transport::Transport;
pub use unread::Unread;
