//! Where a block sends its log lines.

use rastro_collector::{NonEmptyText, Observation};

use crate::collectors::nginx::value_objects::LogKind;

/// One `access_log` or `error_log`, as the block declares it.
///
/// **A log destination is state a fingerprint should carry.** A request log redirected to
/// `off`, or to a syslog server, or to a path nothing rotates, changes what a box can tell
/// you about itself afterwards — and none of it touches a byte of the served content, so
/// nothing else in the document would say it moved.
///
/// The target is kept as written, which covers the three shapes nginx accepts in one field:
/// a path, `off`, and `syslog:server=…`. `detail` is the rest of the directive that says how
/// rather than where — the format name for an access log, the level for an error log.
///
/// Sorted rather than kept in written order: a block may declare several and nginx writes to
/// all of them, so which came first is not state.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct LogDestination {
    pub kind: LogKind,
    pub target: NonEmptyText,
    pub detail: Option<NonEmptyText>,
}

impl From<&LogDestination> for Observation {
    fn from(log: &LogDestination) -> Self {
        Observation::object([
            (
                "detail",
                log.detail
                    .as_ref()
                    .map_or_else(Observation::null, |detail| {
                        Observation::text(detail.as_str())
                    }),
            ),
            ("kind", Observation::from(&log.kind)),
            ("target", Observation::text(log.target.as_str())),
        ])
    }
}
