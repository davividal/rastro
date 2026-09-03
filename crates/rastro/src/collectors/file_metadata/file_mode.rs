//! An entry's permission bits.

/// The permission bits of an entry, as an operator writes them.
///
/// **Octal text rather than the integer the kernel keeps.** `chmod 0640` and a `mode` of
/// `416` are the same fact, and only one of them is the one an operator typed. The
/// PostgreSQL collector meets the same choice from the other side: `pg_settings` reports
/// `data_directory_mode` as `0700` and its raw form as `448`, and the octal is what a diff
/// is readable in.
///
/// The setuid, setgid and sticky bits are kept, which is why this is four digits rather
/// than three: a file that gains setuid has changed in the way an operator most needs to
/// see, and masking to `0o777` would hide exactly that.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct FileMode(u32);

impl FileMode {
    /// Reads the mode out of the kernel's `st_mode`, which also carries the entry's kind
    /// in its high bits.
    pub fn of(raw_mode: u32) -> Self {
        Self(raw_mode & 0o7777)
    }

    pub fn as_str(&self) -> String {
        format!("{:04o}", self.0)
    }
}
