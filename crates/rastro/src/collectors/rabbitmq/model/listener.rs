//! One socket a node says it is accepting connections on.

use rastro_collector::Observation;

/// A listener as the node reports it, which is not what the box observed bound.
///
/// `design.md` states the rule this follows: the endpoint a service is configured with is a
/// different fact from the one the kernel has a socket for, and the two are separate so they
/// can disagree. The `sockets` facet reports the second; this is the first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listener {
    /// What speaks on it: `amqp`, `clustering`, `http` for the management plugin.
    pub protocol: String,

    /// The address as the node spells it, `[::]` for a dual-stack wildcard.
    pub interface: String,

    pub port: u16,

    /// The node's own description of what the listener is for.
    ///
    /// Kept because it is the node explaining its own configuration, and because the
    /// clustering listener's purpose names the CLI tools: an operator reading the facet can
    /// see which port an administrative connection would use.
    pub purpose: Option<String>,
}

impl From<&Listener> for Observation {
    fn from(listener: &Listener) -> Self {
        Observation::object([
            ("protocol", Observation::text(listener.protocol.as_str())),
            ("interface", Observation::text(listener.interface.as_str())),
            ("port", Observation::integer(i64::from(listener.port))),
            (
                "purpose",
                match &listener.purpose {
                    Some(purpose) => Observation::text(purpose.as_str()),
                    None => Observation::null(),
                },
            ),
        ])
    }
}
