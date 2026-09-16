//! What PAM puts into a login session's environment.
//!
//! Three layers, and the dependency arrows only point one way: [`source`] knows `model`,
//! `model` knows its value objects, and neither of the last two knows a host interface
//! exists.
//!
//! # Why PAM has a collector rather than these files living under `accounts`
//!
//! **Environment belongs to its carrier**, which is the rule that puts a unit's
//! `Environment=` in [`units`](crate::collectors::units) and a crontab's variables in
//! [`cron`](crate::collectors::cron). `/etc/environment` and `/etc/security/pam_env.conf`
//! are read by `pam_env.so` during session setup — by PAM, not by the shell and not by
//! anything that reads `/etc/passwd`. Putting them in `accounts` would have been placing
//! them next to their nearest neighbour rather than with the thing that reads them, and it
//! would have broken the same rule that justified the other two.
//!
//! So the gap was never "environment has no home", it was "PAM has no collector". This is
//! that collector, scoped for now to the session environment; the rest of the PAM surface —
//! the `pam.d` stack, `limits.conf` — is the obvious next tenant and needs no new facet.
//!
//! # What this does not answer yet
//!
//! **`envfile=` sources named in the PAM stack are not discovered.** Debian's `/etc/pam.d/su`
//! carries a second `pam_env.so` line reading `/etc/default/locale`, and that file is where
//! `LANG` actually comes from on most Debian boxes. Finding it means parsing the `pam.d`
//! stack, which is the next step rather than this one — so this facet reports the two
//! sources `pam_env` reads by default and says so rather than implying completeness.

pub mod model;
pub mod source;

pub use model::{FileStatus, SessionEnvironment};
pub use source::{EnvironmentAssignments, environment_file};

// One import, because `rastro-collector` re-exports what an author needs. A
// collector written outside this repo looks exactly like this.
use rastro_collector::{
    CollectionError, Collector, CollectorCategory, CollectorId, CollectorIdentity,
    CollectorVersion, FacetName, Observation, Presence,
};

use crate::collectors::pam::source::session_environment;

/// Where PAM's configuration lives, and the marker that the box uses PAM at all.
const PAM_DIRECTORY: &str = "/etc/pam.d";

pub struct PamCollector {
    name: FacetName,
    identity: CollectorIdentity,
    directory: String,
}

impl PamCollector {
    pub fn new() -> Self {
        Self::reading(PAM_DIRECTORY)
    }

    /// The same collector over a configuration root the caller chose, so both answers of
    /// [`Self::presence`] are reachable from a test.
    pub fn reading(directory: impl Into<String>) -> Self {
        Self {
            name: FacetName::new("pam").expect("`pam` is a legal facet name"),
            identity: CollectorIdentity::new(
                CollectorId::new("pam").expect("`pam` is a legal collector id"),
                CollectorVersion::new("1").expect("`1` is a legal collector version"),
            ),
            directory: directory.into(),
        }
    }
}

impl Default for PamCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl Collector for PamCollector {
    fn name(&self) -> &FacetName {
        &self.name
    }

    fn identity(&self) -> &CollectorIdentity {
        &self.identity
    }

    fn category(&self) -> CollectorCategory {
        CollectorCategory::State
    }

    /// Two answers, and `absent` is a genuine statement about the host.
    ///
    /// `/etc/pam.d` is PAM's configuration root and exists on any box that uses PAM. A box
    /// without it does not run PAM, so there is no session environment for PAM to set — the
    /// same shape of answer the `units` collector gives for a box with no systemd, and
    /// diffable in the direction that matters, since installing PAM flips the facet from
    /// absent to a pair of sources.
    ///
    /// **A box with no PAM may still have an `/etc/environment`**, and this facet stays
    /// silent about it on purpose: that file would then be read by something else, and
    /// reporting it here would claim PAM does something it does not.
    fn presence(&self) -> Presence {
        match std::path::Path::new(&self.directory).is_dir() {
            true => Presence::Present,
            false => Presence::Absent,
        }
    }

    fn collect(&self) -> Result<Observation, CollectionError> {
        Ok(Observation::from(&session_environment::read()))
    }
}
