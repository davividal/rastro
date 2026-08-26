//! The leaves of the exporters facet.
//!
//! A host and a port are not among them: they are shared with the sockets facet and live
//! in [`inet`](crate::collectors::inet), because the two facets exist to be read against
//! each other and must spell an endpoint the same way.

mod agent_id;
mod build_revision;
mod exporter_version;
mod setting_name;

pub use agent_id::AgentId;
pub use build_revision::BuildRevision;
pub use exporter_version::ExporterVersion;
pub use setting_name::SettingName;

pub use rastro_collector::SettingValue;
