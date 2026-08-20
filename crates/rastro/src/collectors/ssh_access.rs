//! Who can log in to this box over ssh.
//!
//! Three layers, and the dependency arrows only point one way:
//! [`source`] knows [`model`], `model` knows [`value_objects`], and neither of the last
//! two knows a host interface exists.
//!
//! # The facet the accounts one cannot be
//!
//! On the box this was developed against, **every account's password is either a placeholder
//! or locked**: not one of seventy-nine can log in with a password. So the `accounts` facet,
//! for all its detail about password states, says almost nothing about who can actually get
//! in. This facet does: the answer is a set of public keys in files that facet never opens.
//!
//! # Why the key body is recorded when a password hash is not
//!
//! The two decisions look contradictory and are not. A password hash is a credential verifier,
//! so publishing one enables offline cracking, and rastro has no redaction layer to hand it
//! to. A public key is public by construction and grants nothing to whoever holds it — and it
//! *is* the access grant, so a key swapped under an unchanged comment and algorithm is
//! invisible in every other field. The full reasoning is on
//! [`value_objects::PublicKey`].
//!
//! # The server settings are here too, because either half misleads alone
//!
//! A hundred authorized keys mean nothing when `PubkeyAuthentication` is `no`, and an empty
//! key file means nothing when `PasswordAuthentication` is `yes`. `sshd -T` is asked, which is
//! OpenSSH resolving its own configuration including every drop-in and `Match` block, so this
//! is effective state rather than a read of `sshd_config`.
pub mod model;
pub mod source;
pub mod value_objects;

pub use model::{AuthorizedKey, SshAccess, SshServer};
pub use source::{SshFiles, Sshd, authorized_keys, resolve};
pub use value_objects::{KeyComment, KeyOption, KeyType, PublicKey, SettingValue};

// One import, because `rastro-collector` re-exports what an author needs. A
// collector written outside this repo looks exactly like this.
use rastro_collector::{
    CollectionError, Collector, CollectorCategory, CollectorId, CollectorIdentity,
    CollectorVersion, FacetName, Observation, Presence,
};

pub struct SshAccessCollector {
    name: FacetName,
    identity: CollectorIdentity,
    files: Option<SshFiles>,
}

impl SshAccessCollector {
    pub fn new() -> Self {
        Self::reading(SshFiles::detect())
    }

    /// The same collector over a source the caller chose.
    pub fn reading(files: Option<SshFiles>) -> Self {
        Self {
            name: FacetName::new("ssh_access").expect("`ssh_access` is a legal facet name"),
            identity: CollectorIdentity::new(
                CollectorId::new("ssh_access").expect("`ssh_access` is a legal collector id"),
                CollectorVersion::new("1").expect("`1` is a legal collector version"),
            ),
            files,
        }
    }
}

impl Default for SshAccessCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl Collector for SshAccessCollector {
    fn name(&self) -> &FacetName {
        &self.name
    }

    fn identity(&self) -> &CollectorIdentity {
        &self.identity
    }

    fn category(&self) -> CollectorCategory {
        CollectorCategory::State
    }

    /// `absent` without an `sshd`, and here that is exact rather than a hedge.
    ///
    /// A box with no ssh server genuinely admits nobody over ssh — the keys in a home
    /// directory grant nothing with no daemon to honour them — so this is the same kind of
    /// answer the units collector gives for a box with no systemd, and not the
    /// `undetermined` the sockets collector gives for a missing `ss`. There is no second
    /// implementation rastro might have failed to look for.
    fn presence(&self) -> Presence {
        match self.files {
            Some(_) => Presence::Present,
            None => Presence::Absent,
        }
    }

    fn collect(&self) -> Result<Observation, CollectionError> {
        let files = self.files.as_ref().ok_or_else(|| {
            CollectionError::new("no ssh server was found, so there is no ssh access to read")
        })?;

        Ok(Observation::from(&files.read()?))
    }
}
