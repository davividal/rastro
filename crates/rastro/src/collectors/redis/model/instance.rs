//! One running server: where it listens, and what it said.

use rastro_collector::Observation;

use crate::collectors::redis::value_objects::{Listener, ServerKind};

/// A server process on this box.
///
/// **An instance that could not be read is still an instance**, with `error` saying why. A
/// server rastro could see and not ask is neither absent nor empty, and a document that dropped
/// it, or rendered it as a reading with nothing in it, would let a before-and-after diff show no
/// change where the truth is "unknown".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instance {
    pub server: ServerKind,

    /// Every socket the server holds, as bound.
    pub listening: Vec<Listener>,

    /// What this run could not find out about the server, if anything.
    pub error: Option<String>,
}

impl From<&Instance> for Observation {
    fn from(instance: &Instance) -> Self {
        Observation::object([
            ("server", Observation::text(instance.server.as_str())),
            (
                "listening",
                Observation::list(instance.listening.iter().map(Observation::from)),
            ),
            (
                "error",
                match &instance.error {
                    Some(error) => Observation::text(error.as_str()),
                    None => Observation::null(),
                },
            ),
        ])
        .incomplete_when(instance.error.is_some())
    }
}
