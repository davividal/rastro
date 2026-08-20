//! Every module the host has loaded.

use std::collections::BTreeMap;

use rastro_collector::{CollectionError, Observation};

use super::kernel_module::KernelModule;
use crate::collectors::modules::value_objects::ModuleName;

/// The loaded modules, keyed by name.
///
/// Keyed rather than listed, which is the opposite call from the mount table. The
/// kernel enforces unique module names at load time, so keying loses nothing, and it
/// buys two things a list would not: ordering becomes structural through the
/// `BTreeMap`, and loading one module shows up as a single added key instead of the
/// block move that the kernel's own most-recently-loaded-first order would produce.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ModuleTable(BTreeMap<ModuleName, KernelModule>);

impl ModuleTable {
    /// Files each module under its name.
    ///
    /// A repeated name is refused rather than overwritten. The kernel cannot produce
    /// one, so it means rastro misread the table, and silently keeping the last of two
    /// entries would drop a loaded module from a document claiming to be complete.
    pub fn new(
        modules: impl IntoIterator<Item = (ModuleName, KernelModule)>,
    ) -> Result<Self, CollectionError> {
        let mut table = BTreeMap::new();

        for (name, module) in modules {
            if table.insert(name.clone(), module).is_some() {
                return Err(CollectionError::new(format!(
                    "the module {:?} was reported twice, so the table was misread",
                    name.as_str()
                )));
            }
        }

        Ok(Self(table))
    }

    pub fn modules(&self) -> &BTreeMap<ModuleName, KernelModule> {
        &self.0
    }
}

impl From<&ModuleTable> for Observation {
    fn from(table: &ModuleTable) -> Self {
        Observation::object(
            table
                .modules()
                .iter()
                .map(|(name, module)| (name.as_str(), Observation::from(module))),
        )
    }
}
