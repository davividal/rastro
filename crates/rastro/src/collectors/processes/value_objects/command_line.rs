//! What a process was started with.

use rastro_collector::Observation;

/// A process's arguments, as the list they are.
///
/// **A list and not a string, because the kernel gives a list and joining it would be
/// lossy.** `/proc/<pid>/cmdline` separates arguments with NUL bytes precisely so that an
/// argument containing a space is unambiguous. Joining on spaces would make
/// `--filter=a b` indistinguishable from two arguments, and quoting it would invent a shell
/// that was never involved.
///
/// **Empty is meaningful and common.** A kernel thread has no command line at all, so
/// `cmdline` reads back as nothing. That is how `kthreadd` and its children are told apart
/// from userspace, and it is why this is not built on a non-empty text type.
///
/// Ordering is the kernel's, and it is the one list in this collector that must not be
/// sorted: `--port 5432` means something and `5432 --port` does not.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct CommandLine(Vec<String>);

impl CommandLine {
    pub fn new(arguments: impl IntoIterator<Item = String>) -> Self {
        Self(arguments.into_iter().collect())
    }

    pub fn arguments(&self) -> &[String] {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<&CommandLine> for Observation {
    fn from(command_line: &CommandLine) -> Self {
        Observation::list(
            command_line
                .arguments()
                .iter()
                .map(|argument| Observation::text(argument.clone())),
        )
    }
}
