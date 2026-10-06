//! Whether a server sees the same files at the same paths as rastro does.

use std::fs;
use std::path::Path;

/// Where a process's mount namespace is linked from, relative to its `/proc` entry.
const MOUNT_NAMESPACE: &str = "ns/mnt";

/// Whether the server shares rastro's mount namespace, and so means what rastro means by a path.
///
/// A redis in a container, or under systemd's `RootDirectory=`, names paths in a namespace of its
/// own: `/data` measured, which on the host is another directory or none. Where the two cannot be
/// compared the answer is no, the direction in which being wrong costs a missing read rather than
/// a wrong one.
pub fn shares_our_mounts(proc: &Path, process_id: u32) -> bool {
    let theirs = fs::read_link(proc.join(process_id.to_string()).join(MOUNT_NAMESPACE));
    let ours = fs::read_link(proc.join("self").join(MOUNT_NAMESPACE));

    matches!((theirs, ours), (Ok(theirs), Ok(ours)) if theirs == ours)
}
