//! What a collector asserts about a value it observed.
//!
//! Both judgements are the collector's alone to make, and neither can be
//! reconstructed from the value itself: `991` gives no hint that it is a PID,
//! and a string gives no hint that it is a password.

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
