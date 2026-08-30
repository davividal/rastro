//! Layer 1: what is on the disk.
//!
//! The largest state surface a host has, and the one nothing else stands in for. A change
//! that leaves a file behind leaves no other trace: applying an Ansible role to the reference
//! box wrote `/etc/postgresql/17/main/conf.d/10-newrelic.conf`, and until this collector was
//! registered no facet in the document mentioned it.
//!
//! **Scope is every mount but the kernel's own interfaces.** `proc`, `sysfs`, `cgroup2`,
//! `tmpfs` and the rest are named and skipped; everything else is walked, `nfs` and `zfs`
//! included, because both hold real data and neither needs a block device. Each mount is
//! walked separately and every walk stops at every mount point, so nothing is walked twice
//! even where a bind mount shares its device. See [`MountedFilesystems`].
//!
//! **Why a policy table and not a strategy object.** Every way of describing a file changes
//! what a leaf in the document looks like, so each one is a change to the output contract
//! rather than a plug-in point. A closed set the compiler can enumerate is what makes adding
//! one name every renderer, test and document that has to change with it; an open set would
//! let the document's shape depend on which object was installed, and a reader could no
//! longer tell a policy decision from a failure.
//!
//! So the varying part is the *decision*, which is data: an unordered set of rules, resolved
//! by the most specific tree containing the path.

pub mod model;
pub mod source;
pub mod value_objects;

pub use model::{
    FileEntry, FilesystemInventory, PolicyRule, UnreadablePath, WalkPolicy, is_absence,
};
pub use source::{FileTree, MountedFilesystems, WalkBoundaries};
pub use value_objects::{
    CanonicalBytes, ContentPolicy, Detail, DeviceNumber, Digest, DigestAlgorithm, FileKind,
    FileMode, MetadataDigest, NanosecondsSinceEpoch,
};

use std::path::{Path, PathBuf};

// One import, because `rastro-collector` re-exports what an author needs.
use rastro_collector::{
    AbsolutePath, CollectionError, Collector, CollectorCategory, CollectorId, CollectorIdentity,
    CollectorVersion, FacetName, Observation, Presence,
};

/// Reports every entry on every filesystem that holds files.
pub struct FilesystemCollector {
    name: FacetName,
    identity: CollectorIdentity,
    walked: Option<WalkBoundaries>,
    policy: Result<WalkPolicy, CollectionError>,
    observer: Option<PathBuf>,
    detail: Detail,
}

impl FilesystemCollector {
    pub fn new() -> Self {
        Self::of(None, Ok(WalkPolicy::built_in()), None)
    }

    /// The same collector, under a table somebody else resolved, omitting the binary this
    /// run is reading from.
    ///
    /// The table arrives as a `Result` because folding the other collectors' claims into it
    /// can fail, and that failure belongs to this facet: a conflict makes the walk
    /// unanswerable, and nothing else in the document is any less true for it. The
    /// alternative was failing the whole run, which would cost an operator every other
    /// facet over a bug in a collector pair.
    ///
    /// The observer is passed in rather than read here, because the `invocation` facet
    /// reports the same path and the two must not disagree.
    pub fn under(policy: Result<WalkPolicy, CollectionError>, observer: Option<PathBuf>) -> Self {
        Self::of(None, policy, observer)
    }

    /// The same collector over roots the caller names.
    ///
    /// The escape hatch that makes the whole collector testable without a mount: with the
    /// roots given, the walk, the policy and the render can be exercised against a scratch
    /// tree on any host. It reports the running binary like any other file, which is what a
    /// local run does.
    pub fn walking(roots: Vec<AbsolutePath>, policy: WalkPolicy) -> Self {
        Self::of(
            Some(WalkBoundaries::of(roots.clone(), roots)),
            Ok(policy),
            None,
        )
    }

    /// The same over named roots, as a staged run: the running binary is left out.
    ///
    /// What `rastro-ssh` produces, with the roots named so a test can reach it.
    pub fn walking_staged(roots: Vec<AbsolutePath>, policy: WalkPolicy) -> Self {
        Self::of(
            Some(WalkBoundaries::of(roots.clone(), roots)),
            Ok(policy),
            Self::running_binary(),
        )
    }

    /// The same, with the boundaries named separately from the roots.
    ///
    /// What a bind mount looks like to the walk: a directory of the tree being walked that is
    /// also a mount point, so it shares the device and must still stop the walk.
    ///
    /// Reports the running binary, like [`Self::walking`]: only a caller that says it staged
    /// a temporary copy gets the omission, and this constructor is handed no such decision.
    pub fn walking_within(
        roots: Vec<AbsolutePath>,
        boundaries: Vec<AbsolutePath>,
        policy: WalkPolicy,
    ) -> Self {
        Self::of(
            Some(WalkBoundaries::of(roots, boundaries)),
            Ok(policy),
            None,
        )
    }

    /// The executable this run is reading the host from.
    ///
    /// `std::env::current_exe` reads `/proc/self/exe` on Linux, so it is the path the kernel
    /// says is running rather than whatever `argv[0]` claims, and it survives the `mktemp`
    /// name `rastro-ssh` stages the binary under.
    ///
    /// `None` where the kernel will not answer, and then the walk reports the binary like any
    /// other file. A run that cannot tell which file it is should report one entry too many
    /// rather than guess at a path and omit somebody else's.
    ///
    /// Public because the `invocation` facet accounts for the omission and has to name the
    /// same path: one definition, so the two cannot disagree.
    pub fn running_binary() -> Option<PathBuf> {
        std::env::current_exe().ok()
    }

    fn of(
        walked: Option<WalkBoundaries>,
        policy: Result<WalkPolicy, CollectionError>,
        observer: Option<PathBuf>,
    ) -> Self {
        Self {
            name: FacetName::new("filesystem").expect("`filesystem` is a legal facet name"),
            identity: CollectorIdentity::new(
                CollectorId::new("filesystem").expect("`filesystem` is a legal collector id"),
                CollectorVersion::new("1").expect("`1` is a legal collector version"),
            ),
            walked,
            policy,
            observer,
            detail: Detail::Summary,
        }
    }

    /// The same collector, recording every attribute rather than one digest of them.
    ///
    /// A builder rather than a sixth constructor parameter: the detail is a rendering
    /// decision the five ways of constructing this collector all share, and threading it
    /// through each of them would say nothing about any of them.
    pub fn in_detail(mut self, detail: Detail) -> Self {
        self.detail = detail;

        self
    }

    /// The mount points to walk, asked of the kernel unless the caller named them.
    fn walked(&self) -> Result<WalkBoundaries, CollectionError> {
        match &self.walked {
            Some(named) => Ok(named.clone()),
            None => MountedFilesystems::of_this_host().walked(),
        }
    }
}

impl Default for FilesystemCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl Collector for FilesystemCollector {
    fn name(&self) -> &FacetName {
        &self.name
    }

    fn identity(&self) -> &CollectorIdentity {
        &self.identity
    }

    fn category(&self) -> CollectorCategory {
        CollectorCategory::State
    }

    /// Always present: a host running rastro has a filesystem, and rastro was read off one.
    ///
    /// Neither `absent` nor `undetermined` has a meaning here. There is no host where the
    /// subject is missing and nothing to establish, so what can go wrong is the reading, and
    /// that is a failure reported from [`Collector::collect`].
    fn presence(&self) -> Presence {
        Presence::Present
    }

    fn collect(&self) -> Result<Observation, CollectionError> {
        let policy = self.policy.as_ref().map_err(Clone::clone)?;
        let walked = self.walked()?;
        let boundaries: Vec<&Path> = walked
            .boundaries()
            .iter()
            .map(|boundary| Path::new(boundary.as_str()))
            .collect();

        let inventories = walked
            .roots()
            .iter()
            .map(|root| {
                let tree = FileTree::at(Path::new(root.as_str())).stopping_at(&boundaries);

                match &self.observer {
                    Some(binary) => tree.omitting(binary),
                    None => tree,
                }
                .walk(policy)
            })
            .collect::<Result<Vec<FilesystemInventory>, CollectionError>>()?;

        Ok(FilesystemInventory::merged(inventories)?.observation(self.detail))
    }
}
