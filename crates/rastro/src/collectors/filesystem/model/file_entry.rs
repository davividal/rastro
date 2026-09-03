//! One entry on the disk, in rastro's terms.

use rastro_collector::{AbsolutePath, ByteSize, Observation, Xxh3Digest};

use crate::collectors::file_metadata::FileMode;
use crate::collectors::filesystem::value_objects::{
    CanonicalBytes, ContentPolicy, Detail, DeviceNumber, Digest, FileKind,
    NanosecondsSinceEpoch,
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
///
/// **On a directory those two and `link_count` are annotated volatile**, because there
/// they are a summary of entries the walk reports individually rather than an observation
/// of the directory: all three move when a child appears, and the child's own entry is
/// the fact. Read and rendered as always, so the complete view still carries them.
///
/// `reading` is the policy the entry was recorded under, and it is on the entry rather
/// than looked up again at render time because it decides two things here: a tree whose
/// rule says it churns has its `size`, `inode` and both stamps annotated volatile too, and
/// a sealed tree's own directory is where the walk stopped. Which rule that was, and who
/// asked for it, is in the effective table in the `invocation` facet.
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
    pub reading: ContentPolicy,
}

impl FileEntry {
    /// This entry as the document records it.
    pub fn observation(&self, detail: Detail) -> Observation {
        match detail {
            Detail::Summary => Observation::from(&self.metadata_digest()),
            Detail::Full => self.every_attribute(),
        }
    }

    /// A digest over exactly the attributes the diffable view would have kept.
    ///
    /// **Volatility decides what goes in, and it has to.** A digest over a directory's derived
    /// stamps would move whenever a child appeared, and one over a churning tree's size and
    /// inode would move on every run — which would end the byte-identical guarantee that the
    /// whole format rests on, at the one facet that dominates the document.
    ///
    /// The policy the entry was read under is deliberately *not* in here. It is rastro's
    /// configuration rather than the box's state, so folding it in would report a changed
    /// config as a change to every file on the host. The effective table in the `invocation`
    /// facet is where a reader learns which rule applied.
    pub fn metadata_digest(&self) -> Xxh3Digest {
        let derived = self.stamps_are_derived();
        let churns = self.churns();

        CanonicalBytes::new()
            .text(self.kind.as_str())
            .text(&self.mode.as_str())
            .integer(self.owner)
            .integer(self.group)
            .maybe_integer(churns, self.size.as_ref().map(ByteSize::bytes))
            .maybe_integer(derived || churns, Some(self.modified.as_i64()))
            .maybe_integer(derived || churns, Some(self.changed.as_i64()))
            .maybe_integer(churns, Some(self.inode))
            .maybe_integer(derived, Some(self.link_count))
            .maybe_text(false, self.link_target.as_deref())
            .maybe_integer(false, self.device.as_ref().map(DeviceNumber::major))
            .maybe_integer(false, self.device.as_ref().map(DeviceNumber::minor))
            .maybe_text(false, self.digest.as_ref().map(Digest::as_str))
            .digest()
    }

    /// Whether this entry's stamps and link count summarise what is inside it rather than
    /// describing it, in which case the walk reports the change as the child's own entry.
    fn stamps_are_derived(&self) -> bool {
        self.kind.summarises_what_is_inside_it()
    }

    /// Whether the tree this entry was read under was declared to move on its own.
    fn churns(&self) -> bool {
        self.reading.churns()
    }

    fn every_attribute(&self) -> Observation {
        // Two reasons a value here is volatile, and they are different facts: a directory
        // summarises children the walk reports one by one, and a claimed tree was declared
        // to move on its own. A stamp on a directory in a churning tree is both.
        let derived = self.stamps_are_derived();
        let churns = self.churns();
        let volatile_if = |condition: bool, value: Observation| match condition {
            true => value.volatile(),
            false => value,
        };

        Observation::object([
            ("kind", Observation::text(self.kind.as_str())),
            ("mode", Observation::text(self.mode.as_str())),
            ("owner", Observation::integer(self.owner)),
            ("group", Observation::integer(self.group)),
            (
                "size",
                volatile_if(
                    churns,
                    match self.size {
                        Some(size) => Observation::integer(size.bytes()),
                        None => Observation::null(),
                    },
                ),
            ),
            (
                "modified_nanoseconds_since_epoch",
                volatile_if(derived || churns, Observation::from(&self.modified)),
            ),
            (
                "changed_nanoseconds_since_epoch",
                volatile_if(derived || churns, Observation::from(&self.changed)),
            ),
            (
                "inode",
                volatile_if(churns, Observation::integer(self.inode)),
            ),
            (
                "link_count",
                volatile_if(derived, Observation::integer(self.link_count)),
            ),
            (
                "link_target",
                match &self.link_target {
                    Some(target) => Observation::text(target.as_str()),
                    None => Observation::null(),
                },
            ),
            (
                "device",
                match &self.device {
                    Some(device) => Observation::from(device),
                    None => Observation::null(),
                },
            ),
            (
                "digest",
                match &self.digest {
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
