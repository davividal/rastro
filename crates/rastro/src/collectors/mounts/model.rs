//! What rastro reports about mounts.
//!
//! The structure of the facet: types that render as a node composed of other
//! domain types, as opposed to the leaf fields in
//! [`value`](super::value). Nothing here knows which host interface the
//! values came from.

mod mount;
mod mount_table;

pub use mount::Mount;
pub use mount_table::MountTable;
