//! What a collector may say about the trees it owns.
//!
//! The filesystem walk is agnostic by design: it reads every mount that holds files and
//! needs no declaration to find anything. That is its whole value, and it is also why it
//! cannot know that `/var/lib/postgresql/17/main` is a cluster whose catalogues another
//! facet already reports properly, or that reading it means hashing a petabyte.
//!
//! The collector that owns the tree knows both. A claim is how it says so, and it travels
//! through this port so that neither collector depends on the other: the walk consumes
//! claims without knowing who wrote them, and a claimant names a tree without knowing
//! anything about walking.
//!
//! **A claim only ever narrows.** [`ClaimedReading`] cannot spell "hash this", so a claim
//! can reduce what the walk reads and never enlarge it. The config layer follows the same
//! rule by policy; here the type enforces it.

mod claimed_reading;
mod filesystem_claim;

pub use claimed_reading::ClaimedReading;
pub use filesystem_claim::FilesystemClaim;
