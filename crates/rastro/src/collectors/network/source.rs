//! The host interfaces the network facet can be read from.

mod ip;
mod ip_addr;
mod ip_route;

pub use ip::Ip;
pub use ip_addr::{AddressObject, InterfaceObject};
pub use ip_route::RouteObject;
