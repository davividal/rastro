//! Whether a kernel subsystem is already loaded, asked without loading it.
//!
//! **Why a collector would want to know.** Most of what rastro reads is inert: a file, a
//! `/proc` table, a program that prints its own configuration. A few interfaces are not.
//! Querying nftables over netlink makes the kernel autoload `nf_tables`, which pulls
//! `nfnetlink` and `libcrc32c` in behind it, and rastro has then changed the host it was
//! sent to describe. A before-and-after pair taken across such a run reports three module
//! loads that the change under test did not cause.
//!
//! So a collector that would otherwise provoke a subsystem asks here first, and reads it
//! only when the answer is that it is already there. When it is not there, that *is* the
//! observation: a subsystem the kernel has not loaded is holding no state.
//!
//! Both sources are needed. `/proc/modules` knows what is loaded and nothing about a
//! kernel built with the subsystem compiled in; `/boot/config-<release>` knows the
//! configuration and nothing about what is loaded now.
//!
//! `/proc/config.gz` is the other place a kernel may publish its configuration, and it is
//! deliberately not read: Debian does not enable it, and decompressing it would mean a
//! dependency for a file that is usually absent on the target. Its absence lands in
//! [`Residency::Undetermined`], which is the honest answer rather than a guess.

mod kernel_subsystem;
mod residency;

pub use kernel_subsystem::KernelSubsystem;
pub use residency::Residency;

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Where the kernel publishes its loaded modules.
const PROC_MODULES: &str = "/proc/modules";

/// Where the kernel publishes the release string that names its configuration file.
///
/// A file rather than `uname`, so the whole source stays readable without a libc call and
/// a test can point it somewhere else.
const PROC_OSRELEASE: &str = "/proc/sys/kernel/osrelease";

/// Where Debian installs the configuration the running kernel was built with.
const BOOT_CONFIG: &str = "/boot/config-";

/// What a configuration symbol is set to when the subsystem is compiled in.
const BUILT_IN: &str = "=y";

/// The kernel's own answer to "is this already here", as a source rastro can read.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct KernelResidency {
    /// `None` when the loaded-module list could not be read, which is a different state
    /// from a kernel with nothing loaded.
    loaded: Option<BTreeSet<String>>,
    /// `None` when no kernel configuration could be read, which is a different state from
    /// a configuration that mentions nothing.
    built_in: Option<BTreeSet<String>>,
}

impl KernelResidency {
    /// Reads the host's module list and kernel configuration.
    ///
    /// Neither read failing is fatal, and neither is treated as an answer. A source that
    /// could not be read leaves a subsystem [`Residency::Undetermined`] rather than
    /// `Absent`, which is what a caller should act on rather than an error it has no way to
    /// handle.
    pub fn detect() -> Self {
        Self::at(PROC_MODULES, PROC_OSRELEASE, BOOT_CONFIG)
    }

    /// The same over paths the caller chose.
    pub fn at(
        modules: impl AsRef<Path>,
        osrelease: impl AsRef<Path>,
        config_prefix: impl AsRef<Path>,
    ) -> Self {
        let loaded = fs::read_to_string(modules).ok();
        let config = fs::read_to_string(osrelease)
            .ok()
            .and_then(|release| Self::kernel_config(config_prefix.as_ref(), release.trim()));

        Self::parse(loaded.as_deref(), config.as_deref())
    }

    /// Translates both interfaces into the model.
    ///
    /// Separate from [`Self::at`] so the whole grammar is exercised from a fixture, with no
    /// `/proc` to read from.
    pub fn parse(modules: Option<&str>, kernel_config: Option<&str>) -> Self {
        Self {
            loaded: modules.map(|modules| {
                modules
                    .lines()
                    .filter_map(|line| line.split_whitespace().next())
                    .map(str::to_owned)
                    .collect()
            }),
            built_in: kernel_config.map(|config| {
                config
                    .lines()
                    .filter_map(|line| line.trim().strip_suffix(BUILT_IN))
                    .map(str::to_owned)
                    .collect()
            }),
        }
    }

    /// What is known about one subsystem.
    ///
    /// **`Absent` needs both sources.** It is the only answer that asserts a subsystem
    /// cannot be holding state, so it may only be given when rastro has read both what is
    /// loaded and what the kernel was built with. Either source unread leaves the answer
    /// [`Residency::Undetermined`]: a `/proc/modules` that could not be opened says nothing
    /// about what is in it, and reading that silence as an empty list would report a box
    /// with a live firewall as one with no rules.
    pub fn of(&self, subsystem: &KernelSubsystem) -> Residency {
        if self
            .loaded
            .as_ref()
            .is_some_and(|loaded| loaded.contains(subsystem.module()))
        {
            return Residency::Loaded;
        }

        if self
            .built_in
            .as_ref()
            .is_some_and(|built_in| built_in.contains(subsystem.config_symbol()))
        {
            return Residency::BuiltIn;
        }

        match (&self.loaded, &self.built_in) {
            (Some(_), Some(_)) => Residency::Absent,
            _ => Residency::Undetermined,
        }
    }

    /// Whether this subsystem can be read without causing it to load.
    pub fn is_resident(&self, subsystem: &KernelSubsystem) -> bool {
        self.of(subsystem).is_resident()
    }

    fn kernel_config(prefix: &Path, release: &str) -> Option<String> {
        let mut path = PathBuf::from(prefix);
        // A prefix rather than a directory: Debian's file is `config-<release>`, so the
        // last path segment is half a filename and `join` would make it a directory.
        path.as_mut_os_string().push(release);

        fs::read_to_string(path).ok()
    }
}
