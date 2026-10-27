//! The code a server has loaded into itself.

use std::collections::BTreeMap;

use rastro_collector::Observation;

/// One loaded module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Module {
    /// The module's own version number, as the integer it reports.
    pub version: i64,

    /// The shared object it was loaded from, where the server reports one; redis 7 and later do,
    /// except for a module built into the server.
    pub path: Option<String>,

    /// The arguments it was loaded with, where the server reports them.
    ///
    /// `None` is "not reported" and an empty list is "loaded with none", so a server upgraded
    /// into reporting them does not read as a module whose arguments were removed.
    pub args: Option<Vec<String>>,
}

/// Every module a server has loaded, by name.
///
/// State rather than configuration: a module loaded with `MODULE LOAD` at runtime is in no file,
/// which is the same arrangement that makes the settings worth reading from the server.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Modules {
    pub loaded: BTreeMap<String, Module>,
}

impl From<&Modules> for Observation {
    fn from(modules: &Modules) -> Self {
        Observation::object(modules.loaded.iter().map(|(name, module)| {
            (
                name.as_str(),
                Observation::object([
                    ("version", Observation::integer(module.version)),
                    (
                        "path",
                        match &module.path {
                            Some(path) => Observation::text(path.as_str()),
                            None => Observation::null(),
                        },
                    ),
                    (
                        "args",
                        match &module.args {
                            // A sequence: a module reads its arguments by position.
                            // Sensitive whole: a module can take a password as a load argument,
                            // RediSearch documents one, and no value is judged by its name.
                            Some(args) => Observation::sequence(
                                args.iter().map(|arg| Observation::text(arg.as_str())),
                            )
                            .sensitive(),
                            None => Observation::null(),
                        },
                    ),
                ]),
            )
        }))
    }
}
