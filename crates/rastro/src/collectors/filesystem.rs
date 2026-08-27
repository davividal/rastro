//! Layer 1: what is on the disk.
//!
//! `FileTree::at(root).walk(&policy)` returns every entry under a root. The policy is
//! the decision the walk is driven by: for each tree, whether a file's content is read
//! and digested or whether the entry is known by its metadata alone.
//!
//! Not registered in `built_in()` yet, so nothing here reaches the document. The scope
//! and the exclusions are the increments still owed.
//!
//! **Why a table and not a strategy object.** Every way of describing a file changes
//! what a leaf in the document looks like, so each one is a change to the output
//! contract rather than a plug-in point. A closed set the compiler can enumerate is
//! what makes adding one name every renderer, test and document that has to change with
//! it; an open set would let the document's shape depend on which object was installed,
//! and a reader could no longer tell a policy decision from a failure.
//!
//! So the varying part is the *decision*, which is data: an unordered set of rules,
//! resolved by the most specific tree containing the path.

pub mod model;
pub mod source;
pub mod value_objects;

pub use model::{FileEntry, FilesystemInventory, PolicyRule, WalkPolicy};
pub use source::FileTree;
pub use value_objects::{
    ContentPolicy, DeviceNumber, Digest, DigestAlgorithm, FileKind, FileMode,
    NanosecondsSinceEpoch, WalkedTree,
};
