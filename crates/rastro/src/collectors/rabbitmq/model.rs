//! What rastro means by a RabbitMQ installation, as opposed to how epmd prints it.

mod installation;
mod listener;
mod node;
mod node_status;

pub use installation::Installation;
pub use listener::Listener;
pub use node::Node;
pub use node_status::NodeStatus;
