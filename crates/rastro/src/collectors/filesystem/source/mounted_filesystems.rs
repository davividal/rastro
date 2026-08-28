//! Which filesystems hold files, and where every mount begins.
//!
//! **A named list of kernel interfaces, not the kernel's `nodev` marker.** An earlier version
//! took `nodev` in `/proc/filesystems` to mean "holds no files an operator put there", and
//! that is false: `nfs`, `nfs4`, `9p`, `virtiofs`, `fuse`, `overlay` and `zfs` all need no
//! block device and all hold real data. On a box with an NFS data mount that silently skipped
//! it, and on a ZFS-root box it would have found no filesystem at all and failed the facet.
//!
//! So the criterion is an explicit list of the types that *are* kernel interfaces, and the
//! direction of the remaining error is deliberate. A type missing from the list is walked, so
//! being wrong produces noise an operator can see and exclude; the alternative was silence
//! about data. That also matches the config rule the whole tool follows: narrow by exclusion,
//! never widen by inclusion.
//!
//! **Every mount is a boundary, whatever its device.** The mount points are handed to the
//! walk so it stops at each one, because a bind mount shares its device with what it binds:
//! `mount --bind / /mnt/root` leaves both at device 2049, verified on the reference box, and a
//! walk that only compared device numbers walked and hashed the whole root filesystem a second
//! time under `/mnt/root`.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use rastro_collector::{AbsolutePath, CollectionError};

/// Where the kernel lists what is mounted.
const MOUNTS: &str = "/proc/mounts";

/// The filesystem types that are kernel interfaces rather than storage.
///
/// Everything here is a window onto kernel state that another collector reports properly, or
/// scratch space that does not survive a reboot. Nothing here holds a file somebody put there
/// on purpose and expects to find later.
///
/// Kept as a list because there is no flag that means this. `nodev` does not: it marks a type
/// needing no block device, which `nfs` and `zfs` also do not need. A type absent from this
/// list is walked, which is the safe direction to be wrong in.
const KERNEL_INTERFACES: [&str; 23] = [
    "autofs",
    "binfmt_misc",
    "bpf",
    "cgroup",
    "cgroup2",
    "configfs",
    "cpuset",
    "debugfs",
    "devpts",
    "devtmpfs",
    "efivarfs",
    "fusectl",
    "hugetlbfs",
    "mqueue",
    "nsfs",
    "proc",
    "pstore",
    "ramfs",
    "securityfs",
    "selinuxfs",
    "sysfs",
    "tmpfs",
    "tracefs",
];

/// How many fields of a `/proc/mounts` line are read.
///
/// The device, the mount point and the type. The remaining three are options and two zeroes,
/// which the mounts collector reports and this one does not need.
const READ_FIELDS: usize = 3;

/// The kernel's mount table, ready to say what to walk and where to stop.
pub struct MountedFilesystems {
    mounts: PathBuf,
}

impl Default for MountedFilesystems {
    fn default() -> Self {
        Self::of_this_host()
    }
}

/// What the walk needs to know about the mount table.
///
/// Two answers from one read, and they are not the same set: the roots are the filesystems
/// worth walking, while the boundaries are *every* mount, so a walk stops at a pseudo
/// filesystem it must not enter as well as at the next real one it will walk separately.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalkBoundaries {
    roots: Vec<AbsolutePath>,
    boundaries: Vec<AbsolutePath>,
}

impl WalkBoundaries {
    /// The pair as the caller states it, for a collector walking roots it was given.
    pub fn of(roots: Vec<AbsolutePath>, boundaries: Vec<AbsolutePath>) -> Self {
        Self { roots, boundaries }
    }

    pub fn roots(&self) -> &[AbsolutePath] {
        &self.roots
    }

    pub fn boundaries(&self) -> &[AbsolutePath] {
        &self.boundaries
    }
}

impl MountedFilesystems {
    pub fn of_this_host() -> Self {
        Self {
            mounts: PathBuf::from(MOUNTS),
        }
    }

    /// The same over a table the caller names, which is what makes the scope testable.
    pub fn reading(mounts: &Path) -> Self {
        Self {
            mounts: mounts.to_path_buf(),
        }
    }

    /// The filesystems to walk, and every mount point to stop at.
    ///
    /// Both deduplicated and sorted, because a mount point can appear twice: on the reference
    /// box `binfmt_misc` is mounted over the `autofs` that triggers it, at the same path.
    /// Walking a path twice would report every entry under it twice.
    pub fn walked(&self) -> Result<WalkBoundaries, CollectionError> {
        let mut roots = BTreeSet::new();
        let mut boundaries = BTreeSet::new();

        for line in self.read()?.lines() {
            let fields: Vec<&str> = line.split_whitespace().take(READ_FIELDS).collect();

            if fields.len() < READ_FIELDS {
                continue;
            }

            let (mount_point, filesystem) = (fields[1], fields[2]);

            // The kernel escapes a space in a mount point as `\040`, and a path rastro
            // cannot spell back is a path it must not claim to have walked.
            if mount_point.contains('\\') {
                return Err(CollectionError::new(format!(
                    "{mount_point:?} carries an escape the walk does not decode, so what was \
                     walked could not be recorded faithfully"
                )));
            }

            let mount_point = AbsolutePath::new(mount_point, "mount point")?;

            boundaries.insert(mount_point.clone());

            if !KERNEL_INTERFACES.contains(&filesystem) {
                roots.insert(mount_point);
            }
        }

        if roots.is_empty() {
            return Err(CollectionError::new(
                "no mounted filesystem holds files, and the one rastro is running from does",
            ));
        }

        Ok(WalkBoundaries {
            roots: roots.into_iter().collect(),
            boundaries: boundaries.into_iter().collect(),
        })
    }

    fn read(&self) -> Result<String, CollectionError> {
        fs::read_to_string(&self.mounts).map_err(|error| {
            CollectionError::new(format!(
                "{} could not be read: {error}",
                self.mounts.display()
            ))
        })
    }
}
