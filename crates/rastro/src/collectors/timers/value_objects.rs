//! The leaves of the timers facet.
//!
//! A timer's *name*, and the name of the unit it starts, are both systemd unit names and
//! live in [`systemd`](crate::collectors::systemd).

mod microseconds_since_epoch;

pub use microseconds_since_epoch::MicrosecondsSinceEpoch;
