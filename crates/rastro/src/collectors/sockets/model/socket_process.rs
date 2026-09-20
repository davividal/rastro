//! One process holding a socket open.

use rastro_collector::Observation;

/// A process holding a listening socket, as a pid and the descriptor it holds it on.
///
/// **Nameless on purpose.** The name lives on the [`SocketHolder`] this sits under,
/// because the name is the durable half of the join and these two are not: a pid changes
/// every time a service restarts and the descriptor number changes with it.
///
/// [`SocketHolder`]: super::socket_holder::SocketHolder
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SocketProcess {
    pub process_id: i64,
    pub file_descriptor: i64,
}

impl From<&SocketProcess> for Observation {
    fn from(process: &SocketProcess) -> Self {
        Observation::object([
            (
                "file_descriptor",
                Observation::integer(process.file_descriptor),
            ),
            ("process_id", Observation::integer(process.process_id)),
        ])
    }
}
