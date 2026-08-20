//! The vocabulary systemd's collectors share.
//!
//! A unit name is one concept, and more than one facet is keyed by it: `units` reports
//! what is installed and loaded, `timers` reports when a timer next fires and which
//! unit it starts. Giving each its own near-identical newtype would break the
//! one-term-per-concept rule at the first place a reader would notice, since the two
//! facets are read side by side.
//!
//! Shared *here* rather than in `rastro-collector`, which is the port an outside
//! collector author depends on. A path and a byte size belong there because every
//! collector spells them; a systemd unit name is not universal vocabulary, and putting
//! it in the port would tell an author writing an `nginx` collector that it was.
//!
//! The sibling of [`canonical_tool`](super::canonical_tool), which is shared for the
//! same reason: one place to be right rather than one per collector.

mod unit_name;

pub use unit_name::UnitName;
