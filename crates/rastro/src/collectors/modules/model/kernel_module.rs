//! One loaded kernel module.

use rastro_collector::{ByteSize, Observation};

use crate::collectors::modules::value_objects::{
    Dependants, ModuleState, ReferenceCount, Removability, TaintFlags,
};

/// What rastro records about a module that is loaded.
///
/// Nameless on purpose: the name is the key this is filed under in the
/// [`ModuleTable`](super::module_table::ModuleTable), so repeating it inside would
/// give the document two places to disagree.
///
/// The kernel also reports the module's load address, which rastro does not record.
/// It changes on every boot, so it would be pure noise, and it is a kernel text
/// pointer, so publishing it into a document that gets copied off the box and stored
/// would hand over a KASLR offset for nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KernelModule {
    pub size: ByteSize,
    pub state: ModuleState,
    pub dependants: Dependants,
    pub removability: Removability,
    pub taints: TaintFlags,

    /// How many references the kernel holds. Volatile: it moves as unrelated things
    /// attach to and detach from the module, with nothing having changed about the
    /// host, so it is annotated and the diffable view leaves it out.
    pub reference_count: ReferenceCount,
}

impl From<&KernelModule> for Observation {
    fn from(module: &KernelModule) -> Self {
        Observation::object([
            ("size_bytes", Observation::integer(module.size.bytes())),
            ("state", Observation::from(&module.state)),
            ("dependants", Observation::from(&module.dependants)),
            ("removability", Observation::from(&module.removability)),
            ("taints", Observation::from(&module.taints)),
            (
                "reference_count",
                Observation::from(&module.reference_count).volatile(),
            ),
        ])
    }
}
