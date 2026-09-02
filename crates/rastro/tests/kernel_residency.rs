//! Telling whether a kernel subsystem is already there, without causing it to arrive.
//!
//! This exists because asking a subsystem a question is not free: a netlink query against
//! nftables makes the kernel autoload `nf_tables`, and rastro would then be reporting a
//! host it had just changed. Every fixture line here is real output from the development
//! box.

use rastro::collectors::kernel_residency::{KernelResidency, KernelSubsystem, Residency};

/// Real `/proc/modules` lines. `x_tables` carries a dependant, `ip_tables` does not, and
/// the trailing address column is what makes this whitespace-delimited rather than
/// fixed-width.
const LOADED: &str = "\
unix_diag 16384 0 - Live 0x0000000000000000
ip_tables 32768 0 - Live 0x0000000000000000
x_tables 40960 1 ip_tables, Live 0x0000000000000000
";

/// Real `/boot/config-6.1.0-47-arm64` lines. `NF_TABLES` is a module on Debian's kernel
/// and `EXT4_FS` is built in, which is the distinction this file turns on.
const KERNEL_CONFIG: &str = "\
CONFIG_NF_TABLES=m
CONFIG_IP_NF_IPTABLES=m
CONFIG_UNIX_DIAG=m
CONFIG_EXT4_FS=y
";

const NF_TABLES: KernelSubsystem = KernelSubsystem::new("nf_tables", "CONFIG_NF_TABLES");
const IP_TABLES: KernelSubsystem = KernelSubsystem::new("ip_tables", "CONFIG_IP_NF_IPTABLES");
const EXT4: KernelSubsystem = KernelSubsystem::new("ext4", "CONFIG_EXT4_FS");
const UNHEARD_OF: KernelSubsystem = KernelSubsystem::new("wireguard", "CONFIG_WIREGUARD");

fn residency() -> KernelResidency {
    KernelResidency::parse(Some(LOADED), Some(KERNEL_CONFIG))
}

#[test]
fn a_module_in_proc_modules_is_loaded() {
    // Act & Assert
    assert_eq!(residency().of(&IP_TABLES), Residency::Loaded);
}

#[test]
fn a_module_the_kernel_was_built_with_is_resident_without_being_listed() {
    // Arrange: ext4 is `=y`, so it never appears in /proc/modules and is always there.

    // Act & Assert
    assert_eq!(residency().of(&EXT4), Residency::BuiltIn);
}

#[test]
fn a_module_that_is_buildable_but_not_loaded_is_absent() {
    // Arrange: `CONFIG_NF_TABLES=m` and nothing loaded it, so no nftables ruleset can
    // exist. This is the answer that lets the firewall facet stay silent about nftables
    // instead of running a tool that would load it.

    // Act & Assert
    assert_eq!(residency().of(&NF_TABLES), Residency::Absent);
}

#[test]
fn a_module_the_kernel_config_never_mentions_is_absent() {
    // Act & Assert
    assert_eq!(residency().of(&UNHEARD_OF), Residency::Absent);
}

#[test]
fn residency_is_undetermined_when_the_kernel_config_cannot_be_read() {
    // Arrange: a container, or a box whose /boot is not mounted. A subsystem that is not
    // loaded might still be built in, and answering `Absent` there would report a box as
    // unfiltered when rastro simply cannot tell.
    let blind = KernelResidency::parse(Some(LOADED), None);

    // Act & Assert
    assert_eq!(blind.of(&NF_TABLES), Residency::Undetermined);
}

#[test]
fn a_loaded_module_is_resident_even_without_the_kernel_config() {
    // Arrange: the loaded list alone is enough for a positive answer, so a missing config
    // never downgrades what /proc/modules already proved.
    let blind = KernelResidency::parse(Some(LOADED), None);

    // Act & Assert
    assert_eq!(blind.of(&IP_TABLES), Residency::Loaded);
}

#[test]
fn a_resident_subsystem_is_safe_to_ask_and_an_absent_one_is_not() {
    // Arrange: the question every caller actually has, rather than which of the three
    // positive answers it got.
    let residency = residency();

    // Act & Assert
    assert!(residency.is_resident(&IP_TABLES));
    assert!(residency.is_resident(&EXT4));
    assert!(!residency.is_resident(&NF_TABLES));
}

#[test]
fn residency_is_undetermined_when_the_loaded_module_list_cannot_be_read() {
    // Arrange: the mirror of the missing-config case, and the more dangerous one. An
    // unreadable `/proc/modules` says nothing about what is loaded, so treating it as an
    // empty list would classify every buildable subsystem `Absent` and report a box with a
    // live firewall as one with no rules at all.
    let blind = KernelResidency::parse(None, Some(KERNEL_CONFIG));

    // Act & Assert
    assert_eq!(blind.of(&NF_TABLES), Residency::Undetermined);
    assert_eq!(blind.of(&IP_TABLES), Residency::Undetermined);
}

#[test]
fn a_built_in_subsystem_is_still_known_without_the_loaded_module_list() {
    // Arrange: `CONFIG_EXT4_FS=y` is enough on its own. A built-in subsystem never appears
    // in `/proc/modules`, so not being able to read that file costs nothing here.
    let blind = KernelResidency::parse(None, Some(KERNEL_CONFIG));

    // Act & Assert
    assert_eq!(blind.of(&EXT4), Residency::BuiltIn);
}

#[test]
fn absent_needs_both_sources_read() {
    // Act & Assert: `Absent` is the only answer that asserts something cannot exist, so it
    // is the one that needs every source. Either half missing downgrades it.
    assert_eq!(
        KernelResidency::parse(None, None).of(&NF_TABLES),
        Residency::Undetermined
    );
    assert_eq!(residency().of(&NF_TABLES), Residency::Absent);
}
