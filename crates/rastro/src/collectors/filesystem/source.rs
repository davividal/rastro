//! How the disk is read: one module per host interface.

mod file_tree;
mod mounted_filesystems;

pub use file_tree::FileTree;
pub use mounted_filesystems::{MountedFilesystems, WalkBoundaries};
