//! Who exists on this box, and what they belong to.
//!
//! Three layers, and the dependency arrows only point one way:
//! [`source`] knows [`model`], `model` knows [`value_objects`], and neither of the
//! last two knows a host interface exists.
//!
//! # This collector does not collect passwords
//!
//! **No credential ever leaves it.** The shadow database is read, and the only things
//! that come out of it are whether a password exists, whether it is locked, and which
//! scheme hashed it. There is no field anywhere in the model that a hash could be
//! stored in, which is a stronger guarantee than marking one sensitive would be while
//! rastro's redaction layer is still unbuilt.
//!
//! # This collector does not register a password change either
//!
//! **That is the direct cost of the paragraph above, and it is a real gap in what a
//! fingerprint of this box tells you.** Read this before relying on a diff of the
//! `accounts` facet to tell you nothing about authentication changed.
//!
//! A password's hash is the only part of it that changes when the password changes.
//! Because no hash is recorded, changing a password is invisible here: the state
//! stays `usable`, the scheme stays `y`, and two fingerprints taken either side of a
//! `passwd` run are identical in this facet.
//!
//! What *is* visible, and is most of what an audit actually asks:
//!
//! - an account appearing, disappearing, or changing its uid, home or shell;
//! - a password appearing or being removed at all, since that moves the state
//!   between `absent`, `unusable`, `locked` and `usable`;
//! - an account being locked or unlocked, which is the `locked` state;
//! - the hashing scheme changing, which is how a release upgrade migrating SHA-512
//!   to yescrypt shows up;
//! - group membership changing, which is how privilege is actually granted on a box
//!   whose accounts all authenticate by key.
//!
//! **One field narrows the gap without recording a credential:**
//! `last_changed_days_since_epoch`, part of [`PasswordAging`]. `shadow(5)` defines
//! that column as the date of the last password change, and `passwd` rewrites it
//! whenever it writes a new hash, so a password change moves it. The resolution is
//! one day, and two changes on the same day are indistinguishable.
//!
//! Two caveats on leaning on it, because it is weaker than it first looks. It is the
//! documented contract of the file rather than something rastro measured, so a tool
//! that edits the hash column directly and leaves the date alone defeats it. And
//! locking or unlocking an account does *not* move it: `usermod -L` only prefixes the
//! hash, which is why the `locked` state exists separately.
//!
//! So the honest summary is that a password change leaves a dated trace and never a
//! recoverable one. That is deliberate, and it is as far as this collector goes until
//! there is a redaction layer to hand a hash to.

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
