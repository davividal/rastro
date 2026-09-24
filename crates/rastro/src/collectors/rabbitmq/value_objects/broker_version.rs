//! Which RabbitMQ, ordered against the oldest one this facet reads.

/// The oldest release this facet reads, as `major.minor.patch`.
///
/// **3.10, because that is what a stock Debian box runs.** The floor was 3.13 for one release,
/// taken from [endoflife.date](https://endoflife.date/rabbitmq), which tracks what upstream
/// still supports. Distributions lag that by years: Debian 12, current stable, ships 3.10.8,
/// and a floor above it means rastro reads no RabbitMQ at all on the platform it targets
/// first. See `docs/decisions.md`.
pub const FLOOR: BrokerVersion = BrokerVersion {
    major: 3,
    minor: 10,
    patch: 0,
};

/// A broker's own version, as the node reports it.
///
/// Compared rather than matched on: the facet asks the same three questions of every release
/// it reads, and the only decision the number carries is whether the node is old enough to be
/// refused before the fat read is made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct BrokerVersion {
    major: u32,
    minor: u32,
    patch: u32,
}

impl BrokerVersion {
    /// The version a node reports, where it reports one this can be ordered by.
    ///
    /// **A pre-release suffix is dropped, not refused.** `4.1.0-beta.3` is a 4.1.0 for the one
    /// purpose this type serves, and a node running one is far above any floor. Absent means
    /// the string was not a version at all, which the caller reads as "this is not evidence
    /// about a broker" rather than as "this broker is too old".
    pub fn parse(version: &str) -> Option<Self> {
        let number = version.split(['-', '+']).next()?;
        let mut parts = number.split('.');

        let mut component = || parts.next().and_then(|part| part.parse::<u32>().ok());
        let major = component()?;
        let minor = component().unwrap_or_default();
        let patch = component().unwrap_or_default();

        Some(Self {
            major,
            minor,
            patch,
        })
    }
}

impl std::fmt::Display for BrokerVersion {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}
