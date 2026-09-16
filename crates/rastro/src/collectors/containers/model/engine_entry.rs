//! The two halves of an engine's entry that every flavour renders the same way.

use rastro_collector::Observation;

use crate::collectors::containers::value_objects::DaemonStatus;

/// Why nothing answered, or nothing at all where something did.
///
/// **The reason is a field rather than a sentence in the status** so a diff can show a box
/// whose daemon stopped answering for a different reason than it did last week.
pub fn status_reason(status: &DaemonStatus) -> Observation {
    match status.reason() {
        Some(reason) => Observation::text(reason),
        None => Observation::null(),
    }
}

/// What the server said, or null where there was no server to ask.
///
/// Null rather than an empty object, and the distinction is the facet's: an engine whose
/// daemon did not answer has nothing to describe, which is a different fact from an engine
/// that answered and holds nothing.
pub fn optional_server<Server>(server: Option<&Server>) -> Observation
where
    for<'server> Observation: From<&'server Server>,
{
    match server {
        Some(server) => Observation::from(server),
        None => Observation::null(),
    }
}
