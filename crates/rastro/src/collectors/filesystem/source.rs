//! How the disk is read: one module per host interface.

mod file_tree;
mod mounted_filesystems;

pub use file_tree::{FileTree, as_document_integer, open_without_following, sha256_of_stream};
pub use mounted_filesystems::{MountedFilesystems, WalkBoundaries};
