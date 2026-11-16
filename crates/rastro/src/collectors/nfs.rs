//! The kernel NFS server and client: what the server exports and how both halves run.
//!
//! Three layers, and the dependency arrows only point one way:
//! [`source`] knows [`model`], `model` knows [`value_objects`], and neither of the last two
//! knows a host interface exists.

pub mod model;
pub mod source;
pub mod value_objects;

pub use model::{ClientExport, ExportTable, ModuleParameters, SecurityGroup, ServerRuntime};
pub use source::{Etab, ModuleParameterDirectory, NfsdFilesystem};
pub use value_objects::{ExportClient, ExportOption, KernelValue, ParameterName, SecurityFlavour};
