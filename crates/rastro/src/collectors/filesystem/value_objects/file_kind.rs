//! What sort of thing an entry is.

/// The kind of a filesystem entry.
///
/// Exhaustive rather than "file or not", because three of these carry state that no
/// content hash could: a symlink's meaning is entirely its target, a device node's is its
/// major and minor numbers, and a socket or fifo has no content to read at all. A walk that
/// classified everything as file-or-directory would report a symlink as an empty file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FileKind {
    Regular,
    Directory,
    Symlink,
    Fifo,
    Socket,
    BlockDevice,
    CharacterDevice,
}

impl FileKind {
    /// The name the document records.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Regular => "regular",
            Self::Directory => "directory",
            Self::Symlink => "symlink",
            Self::Fifo => "fifo",
            Self::Socket => "socket",
            Self::BlockDevice => "block_device",
            Self::CharacterDevice => "character_device",
        }
    }

    /// Whether content exists to be digested.
    ///
    /// Only a regular file has any. Asking here rather than at each call site keeps the
    /// walk from deciding it twice and disagreeing with itself.
    pub fn has_content(&self) -> bool {
        matches!(self, Self::Regular)
    }

    /// Whether the entry addresses a device, and so carries a major and a minor number.
    pub fn is_device(&self) -> bool {
        matches!(self, Self::BlockDevice | Self::CharacterDevice)
    }

    /// Whether the entry's own stamps and link count are a summary of what is inside it.
    ///
    /// True for a directory and nothing else. A directory's `st_mtim`, `st_ctim` and
    /// `st_nlink` all move when an entry is created or removed under it, and the walk
    /// reports every one of those entries in its own right, so the summary carries no
    /// fact of its own while churning on every change beneath it.
    pub fn summarises_what_is_inside_it(&self) -> bool {
        matches!(self, Self::Directory)
    }
}
