//! Which host directory a node's path is, read from the kernel's mount tables.
//!
//! Found by the second domain review, measured: a node in a container with its data on a named
//! volume was read fine and the walk went through 41 entries under the volume, because the path
//! the node uses is not a host path at all. `/proc/<pid>/mountinfo` says which device that path is
//! mounted from and where inside the device; the host's own `mountinfo` says where that device
//! is mounted on the host. Together they name the host directory, as `docker inspect` would and
//! without asking the engine.

use std::path::{Component, Path, PathBuf};

/// A node's path on the host, and the host directory its mount is, which it must stay under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostPath {
    pub path: PathBuf,
    pub mount: PathBuf,
}

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

/// The host path of `path` as the node sees it, where `path` is on a mount of its own, a volume
/// or a bind, and the host mounts the same device somewhere that holds it.
///
/// Nothing where the path is on the node's own root mount, which is a container's image and the
/// container facet's to account for, or where the host does not mount the device.
///
/// **Spelled plainly first**, found by the security review: compared as text, `<volume>/../x`
/// matched the volume's mount, and the host resolved the `..` outside it. `..` here is taken as
/// the node's kernel takes it at a mount point that is not a symlink, one directory up.
pub fn host_path_of(node_table: &str, host_table: &str, path: &Path) -> Option<HostPath> {
    let path = &plainly(path);
    let node_mount = mounts_in(node_table)
        .into_iter()
        .filter(|mount| path.starts_with(&mount.point))
        .max_by_key(|mount| mount.point.components().count())?;
    if node_mount.point == Path::new("/") {
        return None;
    }

    let on_device = node_mount
        .root
        .join(path.strip_prefix(&node_mount.point).ok()?);
    let host_mount = mounts_in(host_table)
        .into_iter()
        .filter(|mount| mount.device == node_mount.device && on_device.starts_with(&mount.root))
        .max_by_key(|mount| mount.root.components().count())?;

    let on_host = |inside: &Path| {
        inside
            .strip_prefix(&host_mount.root)
            .ok()
            .map(|relative| host_mount.point.join(relative))
    };
    Some(HostPath {
        path: on_host(&on_device)?,
        mount: on_host(&node_mount.root)?,
    })
}

/// `path` with `.` dropped and each `..` taking the component before it, never above the root.
fn plainly(path: &Path) -> PathBuf {
    let mut plain = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                plain.pop();
            }
            other => plain.push(other),
        }
    }
    plain
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
