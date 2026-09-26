//! What rastro means by a redis installation, as opposed to how a server spells it.

mod installation;
mod instance;
mod server_identity;

pub use installation::Installation;
pub use instance::Instance;
pub use server_identity::ServerIdentity;
