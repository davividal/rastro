//! The leaves of the time facet.
//!
//! There is no clock-reading type here any more. `WallClock` existed to hold
//! `timedatectl`'s rendering of the current time, and reading the current time is what
//! this collector no longer does: see the note on the collector about the unit that call
//! used to start.

mod timezone;

pub use timezone::Timezone;
