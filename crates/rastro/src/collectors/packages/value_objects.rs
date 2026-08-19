//! The typed fields a package is described by.

mod architecture;
mod error_flag;
mod installation_state;
mod package_manager;
mod package_name;
mod package_version;
mod selection_state;

pub use architecture::Architecture;
pub use error_flag::ErrorFlag;
pub use installation_state::InstallationState;
pub use package_manager::PackageManager;
pub use package_name::PackageName;
pub use package_version::PackageVersion;
pub use selection_state::SelectionState;
