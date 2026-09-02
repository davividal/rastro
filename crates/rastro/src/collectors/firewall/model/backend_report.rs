//! What one packet-filter interface had to say.

use rastro_collector::Observation;

use super::ruleset::Ruleset;

/// One interface's answer, including the two ways of having no rules to report.
///
/// **Three answers, because "no rules" arrives by two different routes and a reader has to
/// be able to tell them apart.** [`Self::Read`] with an empty ruleset is a tool that ran
/// and found nothing. [`Self::SubsystemAbsent`] is a kernel that has not loaded the
/// subsystem at all, so nothing can be holding rules in it: the same outcome, established
/// without running anything, which is the point.
///
/// [`Self::Unreadable`] is the loud one. It means rules may well exist and rastro could not
/// see them, either because the subsystem is resident and the dumping program is missing,
/// or because the kernel configuration could not be read and an unloaded subsystem might
/// still be compiled in. Reporting either of those as "no rules" would describe a filtered
/// box as an open one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendReport {
    /// The interface was dumped. The ruleset may still be empty.
    Read(Ruleset),

    /// The kernel has not loaded this subsystem, so it holds no ruleset.
    SubsystemAbsent,

    /// rastro could not find out, and says why.
    Unreadable(String),
}

impl From<&BackendReport> for Observation {
    fn from(report: &BackendReport) -> Self {
        // The same three keys whichever answer this is, so a consumer never meets a key
        // that is sometimes missing. Which answer it is stays readable from `status`.
        let (status, reason, tables) = match report {
            BackendReport::Read(ruleset) => ("ok", Observation::null(), Observation::from(ruleset)),
            BackendReport::SubsystemAbsent => (
                "absent",
                Observation::null(),
                Observation::from(&Ruleset::default()),
            ),
            BackendReport::Unreadable(reason) => {
                ("error", Observation::text(reason), Observation::null())
            }
        };

        Observation::object([
            ("reason", reason),
            ("status", Observation::text(status)),
            ("tables", tables),
        ])
    }
}
