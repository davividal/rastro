//! Whether a kernel subsystem is already there.

/// What rastro can say about a subsystem without provoking it.
///
/// **The two negative answers are not the same answer, and conflating them is the whole
/// hazard this enum exists to prevent.** `Absent` is a fact: the kernel can build this
/// subsystem as a module, nothing has loaded it, so no state can exist inside it.
/// `Undetermined` is an admission: rastro could not read the kernel's configuration, so a
/// subsystem that is not loaded might still be compiled in and holding state. Reporting the
/// second as the first would describe an unread firewall as an absent one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Residency {
    /// Listed in `/proc/modules`.
    Loaded,

    /// Compiled into the kernel, so it is always present and never listed.
    BuiltIn,

    /// Buildable as a module and not loaded, so nothing can be holding state in it.
    Absent,

    /// No readable kernel configuration, so being unloaded proves nothing.
    Undetermined,
}

impl Residency {
    /// Whether asking this subsystem a question can be done without loading it.
    ///
    /// `Undetermined` answers `false`, because the point of asking is to avoid a load and
    /// an unknown is exactly the case where a load might happen.
    pub fn is_resident(&self) -> bool {
        matches!(self, Self::Loaded | Self::BuiltIn)
    }
}
