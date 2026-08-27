//! One entry on the disk, in rastro's terms.

use rastro_collector::{AbsolutePath, ByteSize, Observation};

use crate::collectors::filesystem::value_objects::{
    DeviceNumber, Digest, FileKind, FileMode, NanosecondsSinceEpoch,
};

/// What the walk recorded about one path.
///
/// A plain aggregate: every field is already a validated value. Which fields are `None`
/// is meaning rather than absence of data, and there are four cases:
///
/// - `digest` is absent when the entry has no content to read (anything but a regular
///   file) or when the policy for its tree is metadata-only. The effective table travels
///   in the envelope, which is what lets a reader tell those two apart.
/// - `size` is absent for the same "no content" reason. A directory's size is the space
///   its own index takes, which changes as entries come and go, and the entries are
///   reported individually anyway.
/// - `link_target` is present only for a symlink, where it *is* the state.
/// - `device` is present only for a block or character device node, where the numbers are
///   the only content there is.
///
/// `inode` and `link_count` are carried because a hardlink is otherwise invisible: two
/// paths sharing an inode are one file, and reporting them as two independent entries
/// would misrepresent both. They also make a rewrite legible — a tool that replaces a file
/// by rename leaves the content identical and the inode different.
///
/// `modified` and `changed` are `st_mtim` and `st_ctim`. They overlap with the rest on
/// purpose: a same-size edit under a metadata-only policy moves the mtime and nothing
/// else, and the ctime is the one an operator cannot set, so a backdating `touch -d`
/// leaves the two disagreeing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    pub path: AbsolutePath,
    pub kind: FileKind,
    pub mode: FileMode,
    pub owner: i64,
    pub group: i64,
    pub size: Option<ByteSize>,
    pub modified: NanosecondsSinceEpoch,
    pub changed: NanosecondsSinceEpoch,
    pub inode: i64,
    pub link_count: i64,
    pub link_target: Option<String>,
    pub device: Option<DeviceNumber>,
    pub digest: Option<Digest>,
}

impl From<&FileEntry> for Observation {
    fn from(entry: &FileEntry) -> Self {
        Observation::object([
            ("kind", Observation::text(entry.kind.as_str())),
            ("mode", Observation::text(entry.mode.as_str())),
            ("owner", Observation::integer(entry.owner)),
            ("group", Observation::integer(entry.group)),
            (
                "size",
                match entry.size {
                    Some(size) => Observation::integer(size.bytes()),
                    None => Observation::null(),
                },
            ),
            (
                "modified_nanoseconds_since_epoch",
                Observation::from(&entry.modified),
            ),
            (
                "changed_nanoseconds_since_epoch",
                Observation::from(&entry.changed),
            ),
            ("inode", Observation::integer(entry.inode)),
            ("link_count", Observation::integer(entry.link_count)),
            (
                "link_target",
                match &entry.link_target {
                    Some(target) => Observation::text(target.as_str()),
                    None => Observation::null(),
                },
            ),
            (
                "device",
                match &entry.device {
                    Some(device) => Observation::from(device),
                    None => Observation::null(),
                },
            ),
            (
                "digest",
                match &entry.digest {
                    Some(digest) => Observation::object([
                        ("algorithm", Observation::text(digest.algorithm().as_str())),
                        ("value", Observation::text(digest.as_str())),
                    ]),
                    None => Observation::null(),
                },
            ),
        ])
    }
}
