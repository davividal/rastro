//! Who exists on this box, and what they belong to.
//!
//! Three layers, and the dependency arrows only point one way:
//! [`source`] knows [`model`], `model` knows [`value_objects`], and neither of the
//! last two knows a host interface exists.
//!
//! **No credential ever leaves this collector.** The shadow database is read, and
//! what comes out of it is whether a password exists, whether it is locked, and which
//! scheme hashed it. There is no field anywhere in the model that a hash could be
//! stored in, which is a stronger guarantee than marking one sensitive would be while
//! rastro's redaction layer is still unbuilt.

pub mod model;
pub mod source;
pub mod value_objects;

pub use model::{AccountRegistry, GroupAccount, PasswordAging, PasswordStatus, UserAccount};
pub use source::{AccountFiles, EtcGroup, EtcPasswd, PasswdEntry, ShadowDatabase, ShadowEntry};
pub use value_objects::{
    Comment, GroupId, GroupMembers, GroupName, HashAlgorithm, UserId, UserName,
};

// One import, because `rastro-collector` re-exports what an author needs. A
// collector written outside this repo looks exactly like this.
use rastro_collector::{
    CollectionError, Collector, CollectorCategory, CollectorId, CollectorIdentity,
    CollectorVersion, FacetName, Observation, Presence,
};

pub struct AccountsCollector {
    name: FacetName,
    identity: CollectorIdentity,
    files: AccountFiles,
}

impl AccountsCollector {
    pub fn new() -> Self {
        Self::reading(AccountFiles::new())
    }

    /// The same collector over a source the caller chose, so both answers of
    /// [`Self::presence`] and the failure paths of [`Self::collect`] are reachable
    /// from a test.
    pub fn reading(files: AccountFiles) -> Self {
        Self {
            name: FacetName::new("accounts").expect("`accounts` is a legal facet name"),
            identity: CollectorIdentity::new(
                CollectorId::new("accounts").expect("`accounts` is a legal collector id"),
                CollectorVersion::new("1").expect("`1` is a legal collector version"),
            ),
            files,
        }
    }
}

impl Default for AccountsCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl Collector for AccountsCollector {
    fn name(&self) -> &FacetName {
        &self.name
    }

    fn identity(&self) -> &CollectorIdentity {
        &self.identity
    }

    fn category(&self) -> CollectorCategory {
        CollectorCategory::State
    }

    /// Two answers, not three, and the difference from the kernel collectors is the
    /// filesystem these files live on.
    ///
    /// `/proc` is a pseudo-filesystem, so a missing entry under it is ambiguous
    /// between "the kernel does not offer this" and "nothing is mounted here", and the
    /// module and sysctl collectors need a third answer to say which. `/etc` is
    /// ordinary storage: either the file is there or the host keeps no local account
    /// database, and an image built from scratch really does keep none.
    fn presence(&self) -> Presence {
        if self.files.exists() {
            return Presence::Present;
        }

        Presence::Absent
    }

    fn collect(&self) -> Result<Observation, CollectionError> {
        Ok(Observation::from(&self.files.read()?))
    }
}
