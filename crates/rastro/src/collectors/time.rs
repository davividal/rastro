//! How this box keeps time.
//!
//! Three layers, and the dependency arrows only point one way:
//! [`source`] knows [`model`], `model` knows [`value_objects`], and neither of the last
//! two knows a host interface exists.
//!
//! **A small facet carrying one very large fact.** The timezone moves every log timestamp,
//! every cron schedule and every `OnCalendar=` timer on the box, and it is one symlink.
//!
//! # Why this reads files, when `timedatectl` exists and would be the effective state
//!
//! It used to run `timedatectl show`, which is the canonical tool and reports two settings
//! the files do not. That was reversed, and the reason is worth stating in full because it
//! is the only place in rastro where the tool of choice had to be given up.
//!
//! **`timedatectl` starts a systemd unit on the box being fingerprinted.**
//! `systemd-timedated.service` is `Type=dbus`, so the first D-Bus call activates it, and it
//! then keeps running. Measured rather than reasoned: with the unit stopped,
//! `systemctl list-unit-files` left it `inactive` and a single `timedatectl show` left it
//! `active`.
//!
//! Two things follow, and either alone would be enough.
//!
//! - **A fingerprint must not change the box.** rastro runs as root on production to
//!   observe, and starting a unit is a mutation, however small. Nothing else rastro runs
//!   does it: `systemctl`, `ss`, `ip`, `lsblk`, `iptables-save`, `dpkg-query` and `sshd -T`
//!   all leave the box as they found it.
//! - **It broke the determinism harness**, which is how it was found. The unit that the
//!   `time` collector started appeared in the *next* run's `processes` facet, so two runs of
//!   an unchanged host differed by one process — and only in CI, where every tool is present
//!   and both runs happen seconds apart.
//!
//! What the files give up is `CanNTP` and `NTP`: whether a time-synchronisation service
//! exists and whether it is switched on. Neither is lost from the document, because both are
//! the enablement state of a unit and that is the `units` facet's answer to give —
//! `systemd-timesyncd.service` is `enabled` there. What is kept is the timezone, the
//! hardware clock's scale, and whether synchronisation has actually happened.
//!
//! Kept apart from the `locale` facet, which reads files for a related reason, so that
//! neither facet is named after half of what it holds.
pub mod model;
pub mod source;
pub mod value_objects;

pub use model::ClockSettings;
pub use source::ClockFiles;
pub use value_objects::Timezone;

// One import, because `rastro-collector` re-exports what an author needs. A
// collector written outside this repo looks exactly like this.
use rastro_collector::{
    CollectionError, Collector, CollectorCategory, CollectorId, CollectorIdentity,
    CollectorVersion, FacetName, Observation, Presence,
};

pub struct TimeCollector {
    name: FacetName,
    identity: CollectorIdentity,
    files: ClockFiles,
}

impl TimeCollector {
    pub fn new() -> Self {
        Self::reading(ClockFiles::new())
    }

    /// The same collector over a source the caller chose.
    pub fn reading(files: ClockFiles) -> Self {
        Self {
            name: FacetName::new("time").expect("`time` is a legal facet name"),
            identity: CollectorIdentity::new(
                CollectorId::new("time").expect("`time` is a legal collector id"),
                // Second version: the source changed from `timedatectl` to the files, and
                // two of the five fields went with it. A consumer comparing fingerprints
                // across the change needs to see that the collector, not the host, moved.
                CollectorVersion::new("2").expect("`2` is a legal collector version"),
            ),
            files,
        }
    }
}

impl Default for TimeCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl Collector for TimeCollector {
    fn name(&self) -> &FacetName {
        &self.name
    }

    fn identity(&self) -> &CollectorIdentity {
        &self.identity
    }

    fn category(&self) -> CollectorCategory {
        CollectorCategory::State
    }

    /// Always present, because every running host keeps time somehow and the files rastro
    /// reads are always answerable about.
    ///
    /// This is a change from the `undetermined` the `timedatectl` version gave when the tool
    /// was missing, and it is the honest consequence of reading files: a box with none of
    /// them is not a box rastro cannot see, it is a box in UTC with no zone configured,
    /// which the data says exactly.
    fn presence(&self) -> Presence {
        Presence::Present
    }

    fn collect(&self) -> Result<Observation, CollectionError> {
        Ok(Observation::from(&self.files.read()?))
    }
}
