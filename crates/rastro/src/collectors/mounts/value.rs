//! The typed fields a mount is made of.
//!
//! Each renders as a leaf of the facet: a scalar, or a list of scalars. Nothing
//! here knows how a host spells it, which is what keeps the source replaceable:
//! `/proc/self/mountinfo` reports the same concepts in a different format.

mod device;
mod filesystem_type;
mod mount_option;
mod mount_options;
mod mount_point;

pub use device::Device;
pub use filesystem_type::FilesystemType;
pub use mount_option::MountOption;
pub use mount_options::MountOptions;
pub use mount_point::MountPoint;
