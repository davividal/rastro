//! Which host directory a node's path is, read from the kernel's mount tables.
//!
//! Found by the second domain review, measured: a node in a container with its data on a named
//! volume was read fine and the walk went through 41 entries under the volume, because the path
//! the node uses is not a host path at all. `/proc/<pid>/mountinfo` says which device that path is
//! mounted from and where inside the device; the host's own `mountinfo` says where that device
//! is mounted on the host. Together they name the host directory, as `docker inspect` would and
//! without asking the engine.

use std::path::{Path, PathBuf};

/// One line of a `mountinfo` table, the fields this reads.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Mount {
    /// `major:minor`, the device the mount is of.
    device: String,

    /// Where inside that device the mount starts.
    root: PathBuf,

    /// Where the mount is seen, in the namespace the table belongs to.
    point: PathBuf,
}

/// Every host path of `path` as the node sees it, where `path` is on a mount of its own, a volume
/// or a bind, and the host mounts the same device somewhere that holds it.
///
/// Nothing where the path is on the node's own root mount, which is a container's image and the
/// container facet's to account for, or where the host does not mount the device.
pub fn host_paths_of(node_table: &str, host_table: &str, path: &Path) -> Vec<PathBuf> {
    let Some(node_mount) = containing(&mounts_in(node_table), path) else {
        return Vec::new();
    };
    if node_mount.point == Path::new("/") {
        return Vec::new();
    }
    match on_device(&node_mount, path) {
        Some(inside) => seen_at(&mounts_in(host_table), &node_mount.device, &inside),
        None => Vec::new(),
    }
}

/// Every host path that shows `path`, a host path, or a part of it: the host can bind a directory
/// at a second path, and found by review, a walk sealed at one went through the live store at the
/// other. `path` itself among them, where the table holds the mount it is on.
pub fn host_aliases_of(host_table: &str, path: &Path) -> Vec<PathBuf> {
    let mounts = mounts_in(host_table);
    containing(&mounts, path)
        .and_then(|mount| Some((on_device(&mount, path)?, mount.device)))
        .map(|(inside, device)| seen_at(&mounts, &device, &inside))
        .unwrap_or_default()
}

/// The mount `path` is on: the one with the longest mount point that holds it.
fn containing(mounts: &[Mount], path: &Path) -> Option<Mount> {
    mounts
        .iter()
        .filter(|mount| path.starts_with(&mount.point))
        .max_by_key(|mount| mount.point.components().count())
        .cloned()
}

/// Where inside its device `path` is, seen through `mount`.
fn on_device(mount: &Mount, path: &Path) -> Option<PathBuf> {
    Some(mount.root.join(path.strip_prefix(&mount.point).ok()?))
}

/// Where the mounts of `device` show `inside` or a part of it: through a mount of something
/// holding it, `inside` under that mount's point; through a mount of something inside it, the
/// whole of that mount.
fn seen_at(mounts: &[Mount], device: &str, inside: &Path) -> Vec<PathBuf> {
    mounts
        .iter()
        .filter(|mount| mount.device == device)
        .filter_map(|mount| match inside.strip_prefix(&mount.root) {
            Ok(below) => Some(mount.point.join(below)),
            Err(_) => mount.root.starts_with(inside).then(|| mount.point.clone()),
        })
        .collect()
}

fn mounts_in(table: &str) -> Vec<Mount> {
    table
        .lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split(' ').collect();
            Some(Mount {
                device: (*fields.get(2)?).to_owned(),
                root: PathBuf::from(unescaped(fields.get(3)?)),
                point: PathBuf::from(unescaped(fields.get(4)?)),
            })
        })
        .collect()
}

/// A `mountinfo` path: the kernel writes a space, a tab, a line break and a backslash as an
/// octal escape, `\040` for a space.
fn unescaped(field: &str) -> String {
    let mut text = String::with_capacity(field.len());
    let mut rest = field;

    while let Some(at) = rest.find('\\') {
        text.push_str(&rest[..at]);
        let digits = rest.get(at + 1..at + 4);
        match digits.and_then(|digits| u8::from_str_radix(digits, 8).ok()) {
            Some(byte) => {
                text.push(char::from(byte));
                rest = &rest[at + 4..];
            }
            None => {
                text.push('\\');
                rest = &rest[at + 1..];
            }
        }
    }

    text.push_str(rest);
    text
}
