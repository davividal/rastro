//! A packet-filter interface rastro found, and how to read it without provoking it.

use super::iptables_save;
use crate::collectors::canonical_tool::CanonicalTool;
use crate::collectors::firewall::model::BackendReport;
use crate::collectors::firewall::value_objects::FirewallBackend;
use crate::collectors::kernel_residency::{KernelResidency, Residency};

/// One interface, the tool that dumps it, and what the kernel says about its subsystem.
///
/// **The residency is carried rather than looked up on demand**, so every backend is
/// decided against one reading of `/proc/modules`. Deciding them one at a time would let an
/// earlier backend's own dump change the answer for a later one, which is the exact class
/// of self-inflicted change this collector exists to avoid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirewallSource {
    backend: FirewallBackend,
    residency: Residency,
    /// `None` when the dumping program is not on the box.
    tool: Option<CanonicalTool>,
}

impl FirewallSource {
    /// One source per interface, whether or not it can be read.
    ///
    /// Every backend is always returned, because the document's key set is fixed and each
    /// key needs a definite answer.
    pub fn detect_all(residency: &KernelResidency) -> Vec<Self> {
        FirewallBackend::ALL
            .into_iter()
            .map(|backend| {
                let tool = match residency.is_resident(&backend.subsystem()) {
                    // Not located when the subsystem is absent: finding the program is
                    // pointless when the answer is already known, and `located` touches the
                    // filesystem.
                    false => None,
                    true => CanonicalTool::located(backend.program()),
                };

                Self::using(backend, residency.of(&backend.subsystem()), tool)
            })
            .collect()
    }

    /// The same over a residency and tool the caller chose.
    pub fn using(
        backend: FirewallBackend,
        residency: Residency,
        tool: Option<CanonicalTool>,
    ) -> Self {
        Self {
            backend,
            residency,
            tool,
        }
    }

    pub fn backend(&self) -> FirewallBackend {
        self.backend
    }

    /// Dumps the ruleset, or says why it did not.
    ///
    /// No arguments at all, which is the whole point of using the `-save` programs rather
    /// than `iptables -S`: the latter reports one table and would need rastro to know the
    /// list of tables to ask about, while the dump covers every table that exists.
    ///
    /// This returns a report rather than a `Result` because a backend rastro cannot read is
    /// an observation about the box, not a failure of the run. One unreadable interface
    /// must not cost the document the other three.
    pub fn read(&self) -> BackendReport {
        match self.residency {
            Residency::Absent => BackendReport::SubsystemAbsent,
            Residency::Undetermined => BackendReport::Unreadable(format!(
                "the kernel configuration could not be read, so whether the {} subsystem is \
                 present cannot be told, and asking it would load it",
                self.backend.subsystem().module()
            )),
            Residency::Loaded | Residency::BuiltIn => self.dump(),
        }
    }

    fn dump(&self) -> BackendReport {
        let Some(tool) = &self.tool else {
            return BackendReport::Unreadable(format!(
                "the {} subsystem is loaded but {} is not installed, so any rules it holds \
                 cannot be read",
                self.backend.subsystem().module(),
                self.backend.program()
            ));
        };

        match tool.run(&[]).map(|dump| iptables_save::parse(&dump)) {
            Ok(Ok(ruleset)) => BackendReport::Read(ruleset),
            Ok(Err(failure)) | Err(failure) => BackendReport::Unreadable(failure.to_string()),
        }
    }
}
