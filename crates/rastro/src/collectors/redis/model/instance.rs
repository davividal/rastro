//! One running server: where it listens, and what it said.

use rastro_collector::Observation;

use crate::collectors::redis::model::{Accounts, Modules, Replication, ServerIdentity, Settings};
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

    /// The settings the server is running with, where it let them be read.
    pub settings: Option<Settings>,

    /// Its place in replication, where it let that be read.
    pub replication: Option<Replication>,

    /// Its accounts, where the server has them and let them be read.
    ///
    /// Absent without an error on a redis older than accounts, which is a server with nothing
    /// to list rather than one that refused.
    pub acl: Option<Accounts>,

    /// The modules it has loaded, where it let them be read.
    pub modules: Option<Modules>,

    /// Each thing the box withheld from this run, in the order it was asked: no privilege, no
    /// credential or a refused one, a command the server refuses.
    ///
    /// **Not an error**, the rule elasticsearch set: nothing on the box is wrong, and an operator
    /// looking for faults should not find these among them. One reason per withheld read rather
    /// than the first alone: a server with `CONFIG` renamed away may refuse `ACL` too, and each
    /// refusal is a separate fact about its hardening.
    pub not_read: Vec<String>,

    /// Each thing this run tried and failed to find out about the server, in the order it was
    /// asked.
    pub errors: Vec<String>,
}

impl Instance {
    /// The family, from the server's own answer where there is one.
    pub fn server(&self) -> ServerKind {
        self.identity
            .as_ref()
            .map_or(self.process_kind, |identity| identity.kind)
    }

    /// Every failure, as one sentence, where there was any.
    pub fn error(&self) -> Option<String> {
        joined(&self.errors)
    }

    /// Everything withheld, as one sentence, where there was any.
    pub fn not_read(&self) -> Option<String> {
        joined(&self.not_read)
    }
}

fn joined(reasons: &[String]) -> Option<String> {
    match reasons.is_empty() {
        true => None,
        false => Some(reasons.join("; ")),
    }
}

impl From<&Instance> for Observation {
    fn from(instance: &Instance) -> Self {
        let identity = instance.identity.as_ref();
        let text_or_null = |value: Option<&String>| match value {
            Some(value) => Observation::text(value.as_str()),
            None => Observation::null(),
        };

        let unsupported = identity.and_then(ServerIdentity::unsupported);
        let observation = Observation::object([
            ("server", Observation::text(instance.server().as_str())),
            (
                "listening",
                // A set: the kernel lists a server's sockets in no order it acts on.
                Observation::set(instance.listening.iter().map(Observation::from)),
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
            (
                "settings",
                match &instance.settings {
                    Some(settings) => Observation::from(settings),
                    None => Observation::null(),
                },
            ),
            (
                "replication",
                match &instance.replication {
                    Some(replication) => Observation::from(replication),
                    None => Observation::null(),
                },
            ),
            (
                "acl",
                match &instance.acl {
                    Some(acl) => Observation::from(acl),
                    None => Observation::null(),
                },
            ),
            (
                "modules",
                match &instance.modules {
                    Some(modules) => Observation::from(modules),
                    None => Observation::null(),
                },
            ),
            ("unsupported", text_or_null(unsupported.as_ref())),
            ("not_read", text_or_null(instance.not_read().as_ref())),
            ("error", text_or_null(instance.error().as_ref())),
        ]);

        // Read by the supported releases' rules all the same, and said so to the run's summary.
        let best_effort = match unsupported.is_some() {
            true => observation.approximate(),
            false => observation,
        };
        best_effort.incomplete_when(!instance.errors.is_empty() || !instance.not_read.is_empty())
    }
}
