//! What a container runs.

use rastro_collector::{NonEmptyText, Observation};

/// The command as the engine resolved it, not as it was configured.
///
/// **The resolved form is the honest one.** An image carries an entrypoint and a default
/// command, a container may override either, and what actually runs is the engine's
/// resolution of the three. Recording the configured halves as well would put the same fact
/// in the document twice, in a shape that cannot say which of the two the process is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerCommand {
    pub path: NonEmptyText,
    /// Plain text rather than a value object with a rule: an empty argument is a legal
    /// argument, and `sh -c ""` is a command a container is entitled to run.
    pub arguments: Vec<String>,
}

impl From<&ContainerCommand> for Observation {
    fn from(command: &ContainerCommand) -> Self {
        Observation::object([
            (
                "arguments",
                Observation::list(command.arguments.iter().map(Observation::text)),
            ),
            ("path", Observation::text(command.path.as_str())),
        ])
    }
}
