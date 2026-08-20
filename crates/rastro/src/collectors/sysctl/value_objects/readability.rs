//! Whether a parameter holds state at all.

/// Whether anybody at all is allowed to read a parameter.
///
/// **This is the distinction between a setting and a button.** A handful of the
/// entries the kernel publishes under sysctl are not parameters in any sense:
/// writing to `vm.drop_caches`, `vm.compact_memory` or `net.ipv4.route.flush`
/// makes the kernel *do* something, and there is no value to read back. The
/// kernel says so in the only way a filesystem can, by granting no read
/// permission to anyone, and answers `EACCES` even to root.
///
/// So they are dropped rather than recorded, and dropped for the stated reason
/// rather than by name. A name list would be wrong the first time a kernel adds a
/// seventh trigger; the permission bits are the kernel's own answer to the
/// question being asked. Five such entries exist on Debian 12's 6.1 kernel.
///
/// **Why the mode and not the read's failure.** Deciding this from `EACCES`
/// instead would conflate two opposite situations: an entry nobody may read, and
/// an entry *this* process may not read because it is not root. The first holds
/// no state and belongs nowhere; the second holds state rastro failed to see,
/// and hiding it would be the silent omission this project refuses. Reading the
/// mode separates them before the read is even attempted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Readability {
    Readable,
    WriteOnly,
}

/// The read bits for user, group and other together.
const ANY_READ_BIT: u32 = 0o444;

impl Readability {
    /// What a mode says about whether the entry behind it can be read.
    pub fn of_mode(mode: u32) -> Self {
        if mode & ANY_READ_BIT == 0 {
            return Self::WriteOnly;
        }

        Self::Readable
    }

    pub fn holds_state(&self) -> bool {
        matches!(self, Self::Readable)
    }
}
