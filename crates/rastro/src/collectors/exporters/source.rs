//! The host interfaces a telemetry fleet is read through.

mod endpoint_dialect;
mod flags;
mod known_agent;
mod telemetry_fleet;
mod version_dialect;

pub use endpoint_dialect::EndpointDialect;
pub use known_agent::{CATALOGUE, KnownAgent};
pub use telemetry_fleet::{Deployment, TelemetryFleet};
pub use version_dialect::VersionDialect;
