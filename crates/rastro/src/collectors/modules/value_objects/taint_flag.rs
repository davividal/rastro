//! One reason a module taints the kernel.

/// A taint the kernel attributes to a module.
///
/// The letters and their meanings are the twenty in
/// `Documentation/admin-guide/tainted-kernels.rst`, declared here in bit order so a
/// set of them sorts the way the kernel emits them. In practice only the
/// module-related subset ever appears against a module, but the emitting loop in
/// `module_flags_taint` filters by which bits are set rather than by which letters
/// are module-capable, so the whole alphabet is admitted.
///
/// Names rather than raw letters, because a diff reading `+ "unsigned_module"` says
/// what happened where `+ "E"` needs a lookup. That is the difference between a
/// fingerprint an operator can read and one they have to decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TaintFlag {
    ProprietaryModule,
    ForcedModule,
    OutOfSpecificationSystem,
    ForcedUnload,
    MachineCheckException,
    BadPage,
    UserspaceRequested,
    KernelDied,
    AcpiTableOverridden,
    WarningIssued,
    StagingDriver,
    FirmwareWorkaround,
    OutOfTreeModule,
    UnsignedModule,
    SoftLockup,
    LivePatched,
    Auxiliary,
    RandstructPlugin,
    InKernelTest,
    FwctlMutatingDebug,

    /// A letter this build of rastro does not know.
    ///
    /// Recorded rather than refused. Bit 19 was added for `fwctl`, so the alphabet
    /// demonstrably grows, and failing the whole facet because one letter is new
    /// would lose every module on the box to report one unknown character. Passing it
    /// through loses nothing: it still appears in the diff.
    Unrecognised(char),
}

impl TaintFlag {
    pub fn from_letter(letter: char) -> Self {
        match letter {
            'P' => Self::ProprietaryModule,
            'F' => Self::ForcedModule,
            'S' => Self::OutOfSpecificationSystem,
            'R' => Self::ForcedUnload,
            'M' => Self::MachineCheckException,
            'B' => Self::BadPage,
            'U' => Self::UserspaceRequested,
            'D' => Self::KernelDied,
            'A' => Self::AcpiTableOverridden,
            'W' => Self::WarningIssued,
            'C' => Self::StagingDriver,
            'I' => Self::FirmwareWorkaround,
            'O' => Self::OutOfTreeModule,
            'E' => Self::UnsignedModule,
            'L' => Self::SoftLockup,
            'K' => Self::LivePatched,
            'X' => Self::Auxiliary,
            'T' => Self::RandstructPlugin,
            'N' => Self::InKernelTest,
            'J' => Self::FwctlMutatingDebug,
            other => Self::Unrecognised(other),
        }
    }

    /// The name as the document spells it.
    pub fn to_name(&self) -> String {
        let name = match self {
            Self::ProprietaryModule => "proprietary_module",
            Self::ForcedModule => "forced_module",
            Self::OutOfSpecificationSystem => "out_of_specification_system",
            Self::ForcedUnload => "forced_unload",
            Self::MachineCheckException => "machine_check_exception",
            Self::BadPage => "bad_page",
            Self::UserspaceRequested => "userspace_requested",
            Self::KernelDied => "kernel_died",
            Self::AcpiTableOverridden => "acpi_table_overridden",
            Self::WarningIssued => "warning_issued",
            Self::StagingDriver => "staging_driver",
            Self::FirmwareWorkaround => "firmware_workaround",
            Self::OutOfTreeModule => "out_of_tree_module",
            Self::UnsignedModule => "unsigned_module",
            Self::SoftLockup => "soft_lockup",
            Self::LivePatched => "live_patched",
            Self::Auxiliary => "auxiliary",
            Self::RandstructPlugin => "randstruct_plugin",
            Self::InKernelTest => "in_kernel_test",
            Self::FwctlMutatingDebug => "fwctl_mutating_debug",
            Self::Unrecognised(letter) => return format!("unrecognised_{letter}"),
        };

        name.to_owned()
    }
}
