//! One running server: where it listens, and what it said.

use rastro_collector::Observation;

use crate::collectors::redis::model::ServerIdentity;
use crate::collectors::redis::value_objects::{Listener, ServerKind};

/// A server process on this box.
///
/// **An instance that could not be read is still an instance**, with `error` saying why. A
/// server rastro could see and not ask is neither absent nor empty, and a document that dropped
/// it, or rendered it as a reading with nothing in it, would let a before-and-after diff show no
/// change where the truth is "unknown".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instance {
    /// The family the process table says this is, before the server has been asked.
    pub process_kind: ServerKind,

    /// Every socket the server holds, as bound.
    pub listening: Vec<Listener>,

    /// What the server said it is, where it was asked and answered.
    pub identity: Option<ServerIdentity>,

    /// What this run could not find out about the server, if anything.
    pub error: Option<String>,
}

impl Instance {
    /// The family, from the server's own answer where there is one.
    pub fn server(&self) -> ServerKind {
        self.identity
            .as_ref()
            .map_or(self.process_kind, |identity| identity.kind)
    }
}

impl From<&Instance> for Observation {
    fn from(instance: &Instance) -> Self {
        let identity = instance.identity.as_ref();
        let text_or_null = |value: Option<&String>| match value {
            Some(value) => Observation::text(value.as_str()),
            None => Observation::null(),
        };

        Observation::object([
            ("server", Observation::text(instance.server().as_str())),
            (
                "listening",
                Observation::list(instance.listening.iter().map(Observation::from)),
            ),
            (
                "version",
                text_or_null(identity.map(|identity| &identity.version)),
            ),
            (
                "mode",
                text_or_null(identity.and_then(|identity| identity.mode.as_ref())),
            ),
            (
                "executable",
                text_or_null(identity.and_then(|identity| identity.executable.as_ref())),
            ),
            (
                "config_file",
                text_or_null(identity.and_then(|identity| identity.config_file.as_ref())),
            ),
            ("error", text_or_null(instance.error.as_ref())),
        ])
        .incomplete_when(instance.error.is_some())
    }
}
