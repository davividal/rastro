//! The typed fields a loaded module is described by.
//!
//! Each renders as a leaf of the facet: a scalar, or a list of scalars. Nothing here
//! knows how a host spells it.

mod dependants;
mod module_name;
mod module_state;
mod reference_count;
mod removability;
mod taint_flag;
mod taint_flags;

pub use dependants::Dependants;
pub use module_name::ModuleName;
pub use module_state::ModuleState;
pub use reference_count::ReferenceCount;
pub use removability::Removability;
pub use taint_flag::TaintFlag;
pub use taint_flags::TaintFlags;
