//! What holds a socket open, named by the one thing about it that lasts.

use std::collections::BTreeSet;

use rastro_collector::{Observation, ProcessName};

use super::socket_process::SocketProcess;

/// One program holding a listening socket, and every process of it that holds one.
///
/// **Keyed by the name, because the number of processes behind it is not state.** A daemon
/// that forks leaves parent and child holding the same listening descriptor, and how many
/// of them exist at the moment `/proc` is scanned is a fact about that moment. Held flat,
/// the volatile pid and descriptor fall away in the diffable view and each process of one
/// name renders as the same bare `{"name": …}`, so an unchanged host stops producing
/// byte-identical documents. Grouping is what makes the count stop mattering: one holder
/// per name, whatever is underneath it.
///
/// What a diff needs is unchanged by the grouping: `postgres` no longer holding 5432 is a
/// change, `postgres` holding it under a new pid is not.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SocketHolder {
    pub name: ProcessName,
    /// Volatile whole, so the diffable view carries the name and nothing under it.
    pub processes: BTreeSet<SocketProcess>,
}

impl From<&SocketHolder> for Observation {
    fn from(holder: &SocketHolder) -> Self {
        Observation::object([
            ("name", Observation::from(&holder.name)),
            (
                "processes",
                Observation::list(holder.processes.iter().map(Observation::from)).volatile(),
            ),
        ])
    }
}
