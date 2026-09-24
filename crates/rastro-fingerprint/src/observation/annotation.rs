//! What a collector asserts about a value it observed.
//!
//! Every judgement is the collector's alone to make, and none can be
//! reconstructed from the value itself: `991` gives no hint that it is a PID,
//! a string gives no hint that it is a password, and `{"error": …}` does not say
//! whether rastro failed or the box reported a fault of its own.

/// Whether a value changes on its own between two runs of an unchanged host.
///
/// PIDs, counters, uptimes and timestamps are `Volatile`. They are the noise
/// floor that makes a naive diff useless.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Volatility {
    #[default]
    Stable,
    Volatile,
}

/// Whether a value must not be printed as it stands.
///
/// Honoured by the redaction layer, which is not built yet. Recording the
/// judgement now costs nothing and cannot be recovered later.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sensitivity {
    #[default]
    Public,
    Sensitive,
}

/// Whether rastro obtained this item, or is recording that it could not.
///
/// `Incomplete` marks the one node standing for an item rastro was refused or could not
/// record, inside a facet that is otherwise `ok`: a path it was not allowed to stat, a file
/// that would not open, an object that vanished between listing and reading. It is not a
/// fault the box reports about itself, which is state rather than a gap, so a broken
/// `pg_hba.conf` line is never marked. The document is unchanged by it; what reads it is the
/// operator's summary of what the run could not see.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Completeness {
    #[default]
    Complete,
    Incomplete,
}
