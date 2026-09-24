//! Why a node known to be a broker was not asked what it runs.

/// The reason rastro did not put its questions to a node a RabbitMQ process holds.
///
/// Both are gaps in what this run saw, not facts about the node: its status, feature flags
/// and definitions are unknown rather than empty. Recorded so a node of nulls never reads
/// as a node with nothing to report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotAsked {
    /// No `rabbitmqctl` in a system directory to ask with.
    NoClient,
    /// The name the node runs under could not be read, and `rabbitmqctl -n` needs it.
    Nameless,
}

impl NotAsked {
    pub fn reason(&self) -> &'static str {
        match self {
            Self::NoClient => {
                "no rabbitmqctl was found in a system directory, so this broker was not asked \
                 what it runs"
            }
            Self::Nameless => {
                "the name this node runs under could not be read, so it could not be addressed \
                 and was not asked what it runs"
            }
        }
    }
}
