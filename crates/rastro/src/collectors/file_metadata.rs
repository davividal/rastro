//! The vocabulary of a file's own attributes, shared across facets.
//!
//! A permission mode is one concept, and more than one facet reports it: the `filesystem`
//! walk records the mode of every path it visits, and the `nginx` facet records the mode of
//! a private key it was pointed at, because a key that became group-readable is a finding
//! rather than a line in a walk of half a million entries. Giving each its own newtype would
//! break the one-term-per-concept rule at the place a reader would most notice it, since the
//! point of reading both is to see the same file described twice.
//!
//! Shared *here* rather than in `rastro-collector`, for the reason [`inet`](super::inet)
//! gives: the port carries what every collector spells, and a POSIX mode is common but not
//! universal.

mod file_mode;

pub use file_mode::FileMode;
