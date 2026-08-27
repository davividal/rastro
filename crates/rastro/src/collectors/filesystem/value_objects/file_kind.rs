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
}
