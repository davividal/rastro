//! Which telemetry agent this is.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// The name rastro knows an agent by, which is the name of its binary.
///
/// **The binary, not the unit.** `process_exporter.service` starts a program called
/// `process-exporter`, underscore against hyphen, and an operator is free to name a unit
/// anything at all. The binary is what fixes the agent's identity, so it is what this is.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct AgentId(NonEmptyText);

impl AgentId {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "telemetry agent name")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&AgentId> for Observation {
    fn from(agent: &AgentId) -> Self {
        Observation::text(agent.as_str())
    }
}
