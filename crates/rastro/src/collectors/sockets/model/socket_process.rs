//! Which process holds a socket open.

use rastro_collector::Observation;

use crate::collectors::sockets::value_objects::ProcessName;

/// One process holding a listening socket.
///
/// **The name is stable and the other two are not, which is the whole reason all three
/// are kept.** A pid changes every time a service restarts and a file descriptor number
/// changes with it, so both are volatile and neither reaches the diffable view. The name
/// is what survives, and it is what a diff needs: `postgres` no longer holding 5432 is a
/// change, `postgres` holding it under a new pid is not.
///
/// Recording the volatile pair rather than dropping it is what makes
/// `--include-volatile` useful for an operator standing in front of the box.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SocketProcess {
    pub name: ProcessName,
    pub process_id: i64,
    pub file_descriptor: i64,
}

impl From<&SocketProcess> for Observation {
    fn from(process: &SocketProcess) -> Self {
        Observation::object([
            (
                "file_descriptor",
                Observation::integer(process.file_descriptor).volatile(),
            ),
            ("name", Observation::from(&process.name)),
            (
                "process_id",
                Observation::integer(process.process_id).volatile(),
            ),
        ])
    }
}
