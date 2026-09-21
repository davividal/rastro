//! The `/proc` interface: what of the Erlang runtime is already up.
//!
//! **The gate the facet hangs on.** A RabbitMQ CLI tool starts the port mapper daemon when
//! it cannot find one, and the daemon outlives the call, so the question "is there anything
//! here to ask" must be answered without asking. The process table answers both halves of
//! it: whether epmd is resident, and whether any beam on the box booted RabbitMQ.
//!
//! **What this read cannot do**, measured rather than assumed: name the node. The broker's
//! beam holds no environment variables at all, and its argv carries no node name, only the
//! application it booted. The name comes from
//! [the register](super::EpmdRegister), which is why the register is read at all.
//!
//! A process id is a plain `u32` here rather than the `processes` facet's `ProcessId`. That
//! type is another collector's leaf value and stays its own, the way the nginx collector
//! keeps its master's pid to itself.

use std::fs;
use std::path::Path;

/// Where the kernel publishes its process table.
const PROC: &str = "/proc";

/// The argument vector's separator, which is how the kernel writes `cmdline`.
const ARGUMENT_SEPARATOR: char = '\0';

/// The program that keeps the register.
const PORT_MAPPER: &str = "epmd";

/// The tokens that say a beam booted RabbitMQ rather than some other Erlang application.
///
/// `-s <module> <function>` is the boot call, and the module is the application's own name,
/// so `-s rabbit boot` is RabbitMQ and `-s ejabberd boot` is somebody else's node that must
/// not be addressed with a RabbitMQ CLI tool.
const BOOT_CALL: [&str; 3] = ["-s", "rabbit", "boot"];

/// What is already running, so the dispatch can decide whether it may ask anything.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ResidentRuntime {
    port_mapper: bool,
    brokers: Vec<u32>,
}

impl ResidentRuntime {
    /// Reads the box's process table.
    pub fn read() -> Self {
        Self::read_in(Path::new(PROC))
    }

    /// The same over a process table the caller names, so this can be exercised on a fixture.
    ///
    /// **Never fails.** An unreadable `/proc` and an entry that vanished mid-walk both mean
    /// the same thing here, which is that nothing was found to ask, and the caller's next
    /// step is to ask nothing. Reporting that as an error would put the facet in `error` on
    /// a box that simply has no RabbitMQ.
    pub fn read_in(proc: &Path) -> Self {
        let Ok(entries) = fs::read_dir(proc) else {
            return Self::default();
        };

        let mut resident = Self::default();
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(process_id) = process_id_of(&path) else {
                continue;
            };

            let Ok(cmdline) = fs::read_to_string(path.join("cmdline")) else {
                continue;
            };

            let arguments: Vec<&str> = cmdline
                .split(ARGUMENT_SEPARATOR)
                .filter(|argument| !argument.is_empty())
                .collect();

            if is_port_mapper(&arguments) {
                resident.port_mapper = true;
            }

            if boots_rabbit(&arguments) {
                resident.brokers.push(process_id);
            }
        }

        // Sorted, because directory order is the filesystem's and a list order that moves
        // between two runs of an unchanged box is what the document's contract forbids.
        resident.brokers.sort_unstable();

        resident
    }

    /// Whether the register may be read at all.
    pub fn port_mapper_running(&self) -> bool {
        self.port_mapper
    }

    /// The processes that booted RabbitMQ, in ascending order.
    ///
    /// Evidence that a broker is up, and deliberately not evidence of *which* one: the argv
    /// does not say. Two entries mean two nodes on this box, which the register then names.
    pub fn broker_process_ids(&self) -> &[u32] {
        &self.brokers
    }
}

/// The pid a `/proc` entry belongs to, or nothing where the entry is not a process.
///
/// `/proc` carries `self`, `cpuinfo` and a good deal else beside its numbered directories.
fn process_id_of(path: &Path) -> Option<u32> {
    path.file_name()?.to_str()?.parse().ok()
}

/// Whether this argument vector belongs to the port mapper.
///
/// The file name of the program, because the daemon runs from a versioned erts directory
/// whose path nobody should have to predict.
fn is_port_mapper(arguments: &[&str]) -> bool {
    arguments
        .first()
        .map(Path::new)
        .and_then(Path::file_name)
        .is_some_and(|program| program == PORT_MAPPER)
}

/// Whether this argument vector booted RabbitMQ.
///
/// The three boot tokens have to be adjacent and in order: `rabbit` on its own appears in
/// half the paths of a RabbitMQ installation, so a vector merely *containing* it says
/// nothing about which application the VM started.
fn boots_rabbit(arguments: &[&str]) -> bool {
    arguments
        .windows(BOOT_CALL.len())
        .any(|window| window == BOOT_CALL)
}
