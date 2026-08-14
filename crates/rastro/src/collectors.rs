//! The collectors that ship inside the binary.

mod host;
mod invocation;
mod mounts;

pub use host::HostCollector;
pub use invocation::{InvocationCollector, seconds_since_epoch};
pub use mounts::{MountsCollector, parse_mount_table};

use rastro_collector::Collector;

/// Every built-in collector, in the order they are registered.
///
/// Order is irrelevant to the document, which sorts facets by name, and this is
/// the list the composition root hands to the use case.
pub fn built_in() -> Vec<Box<dyn Collector>> {
    vec![
        Box::new(HostCollector::new()),
        Box::new(InvocationCollector::new()),
        Box::new(MountsCollector::new()),
    ]
}
