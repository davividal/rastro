//! The typed fields the walker's policy is made of.
//!
//! Each renders as a leaf: the tree a rule names, and what that rule does about
//! content. Nothing here knows how the walk reaches the host.

mod content_policy;
mod digest_algorithm;
mod walked_tree;

pub use content_policy::ContentPolicy;
pub use digest_algorithm::DigestAlgorithm;
pub use walked_tree::WalkedTree;
