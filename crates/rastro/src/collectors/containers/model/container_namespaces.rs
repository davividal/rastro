//! Which namespaces a container has of its own, and which it shares.

use rastro_collector::{NonEmptyText, Observation};

/// The engine's own word for each namespace the container could have shared.
///
/// **Five one-word fields that decide most of what a container can do to the box.**
/// `--pid host` lets it see and signal every process on the machine; `--userns host` makes
/// root inside it root outside it; `--net host` puts it on the box's own stack, where every
/// port it binds is a port on the host. None of that is visible in a process table, and all
/// of it is one word here.
///
/// Absent where docker writes an empty string, which is a container that chose nothing and
/// took the engine's default. Recording the empty string would claim a namespace mode called
/// nothing.
///
/// The network field holds whichever of two kinds of thing docker put there: a namespace
/// choice (`host`, `none`, `container:<id>`) or the name of a network. They are one field in
/// the engine and stay one field here, because splitting them would mean rastro deciding
/// which kind a value is, and a network may legitimately be called `host`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ContainerNamespaces {
    pub control_group: Option<NonEmptyText>,
    pub interprocess: Option<NonEmptyText>,
    pub network: Option<NonEmptyText>,
    pub process: Option<NonEmptyText>,
    pub user: Option<NonEmptyText>,
}

impl From<&ContainerNamespaces> for Observation {
    fn from(namespaces: &ContainerNamespaces) -> Self {
        Observation::object([
            ("control_group", mode(namespaces.control_group.as_ref())),
            ("interprocess", mode(namespaces.interprocess.as_ref())),
            ("network", mode(namespaces.network.as_ref())),
            ("process", mode(namespaces.process.as_ref())),
            ("user", mode(namespaces.user.as_ref())),
        ])
    }
}

fn mode(value: Option<&NonEmptyText>) -> Observation {
    match value {
        Some(value) => Observation::text(value.as_str()),
        None => Observation::null(),
    }
}
