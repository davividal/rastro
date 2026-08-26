//! Reading `/proc/modules`, without needing a `/proc` to read it from.
//!
//! The fixtures are real output captured from `podman run --rm debian:stable-slim`,
//! not invented, because a kernel print format is not something to recall from
//! memory.

use rastro::collectors::modules::{
    KernelModule, ModuleName, ModuleState, ModuleTable, ModulesCollector, ProcModules,
    ReferenceCount, Removability, TaintFlag,
};
use rastro_collector::{Collector, Presence};
use rastro_fingerprint::{Content, Observation, Scalar, View};

const THREE_MODULES: &str = "\
nft_fib_ipv4 12288 1 nft_fib_inet, Live 0x0000000000000000
nft_fib 12288 3 nft_fib_inet,nft_fib_ipv4,nft_fib_ipv6, Live 0x0000000000000000
nft_ct 24576 2 - Live 0x0000000000000000
";

fn parsed(table: &str) -> ModuleTable {
    ProcModules::parse(table).expect("this table is well formed")
}

fn module<'a>(table: &'a ModuleTable, name: &str) -> &'a KernelModule {
    let name = ModuleName::new(name).expect("a legal module name");
    table
        .modules()
        .get(&name)
        .unwrap_or_else(|| panic!("expected a {name:?} module"))
}

fn dependants_of(module: &KernelModule) -> Vec<&str> {
    module.dependants.iter().map(ModuleName::as_str).collect()
}

fn taints_of(module: &KernelModule) -> Vec<TaintFlag> {
    module.taints.iter().copied().collect()
}

#[test]
fn parse_reads_the_columns_that_describe_state() {
    // Act
    let table = parsed(THREE_MODULES);

    // Assert
    let module = module(&table, "nft_ct");
    assert_eq!(module.size.bytes(), 24576);
    assert_eq!(module.state, ModuleState::Live);
    assert_eq!(module.reference_count, ReferenceCount::Counted(2));
    assert_eq!(module.removability, Removability::Removable);
    assert!(module.taints.is_empty());
}

#[test]
fn parse_keys_the_table_by_module_name() {
    // Act
    let table = parsed(THREE_MODULES);

    // Assert: the kernel enforces unique names, so keying loses nothing and a newly
    // loaded module shows as one added key rather than a block move.
    assert_eq!(
        table
            .modules()
            .keys()
            .map(ModuleName::as_str)
            .collect::<Vec<&str>>(),
        ["nft_ct", "nft_fib", "nft_fib_ipv4"]
    );
}

#[test]
fn parse_reads_the_dependants_column_as_a_sorted_set() {
    // Act
    let table = parsed(THREE_MODULES);

    // Assert: the kernel walks its source list in link order, which carries nothing
    // worth diffing and would churn as unrelated modules load.
    assert_eq!(
        dependants_of(module(&table, "nft_fib")),
        ["nft_fib_inet", "nft_fib_ipv4", "nft_fib_ipv6"]
    );
}

#[test]
fn parse_reads_a_dash_as_no_dependants() {
    // Arrange: the trailing comma means an empty final element, and `-` stands for
    // none. Neither is a module.
    let table = parsed(THREE_MODULES);

    // Assert
    assert!(dependants_of(module(&table, "nft_ct")).is_empty());
}

#[test]
fn parse_reads_permanent_as_removability_not_as_a_dependant() {
    // Arrange: a module with an init and no exit can never be removed, and the kernel
    // says so inside the dependants column.
    let table = parsed("crc32 4096 0 [permanent], Live 0x0000000000000000\n");

    // Assert
    let module = module(&table, "crc32");
    assert_eq!(module.removability, Removability::Permanent);
    assert!(
        dependants_of(module).is_empty(),
        "`[permanent]` is not a module called `[permanent]`"
    );
}

#[test]
fn parse_reads_permanent_alongside_real_dependants() {
    // Arrange: both can appear in the one column.
    let table = parsed("crc32 4096 2 ext4,[permanent], Live 0x0000000000000000\n");

    // Assert
    let module = module(&table, "crc32");
    assert_eq!(module.removability, Removability::Permanent);
    assert_eq!(dependants_of(module), ["ext4"]);
}

#[test]
fn parse_reads_a_dash_reference_count_as_untracked() {
    // Arrange: a kernel without `CONFIG_MODULE_UNLOAD` cannot count, and its stub
    // writes `- -` for the count and the dependants both.
    let table = parsed("ext4 962560 - - Live 0x0000000000000000\n");

    // Assert: "nobody is using this" and "this kernel does not count" are different
    // facts.
    assert_eq!(
        module(&table, "ext4").reference_count,
        ReferenceCount::NotTracked
    );
}

#[test]
fn parse_reads_a_negative_reference_count() {
    // Arrange: the kernel reports -1 while a module is unloading.
    let table = parsed("ext4 962560 -1 - Unloading 0x0000000000000000\n");

    // Assert
    let module = module(&table, "ext4");
    assert_eq!(module.reference_count, ReferenceCount::Counted(-1));
    assert_eq!(module.state, ModuleState::Unloading);
}

#[test]
fn parse_names_the_taint_letters_it_knows() {
    // Arrange: an out-of-tree unsigned module, which is exactly the change this tool
    // exists to surface.
    let table = parsed("vboxdrv 663552 3 - Live 0x0000000000000000 (OE)\n");

    // Assert
    assert_eq!(
        taints_of(module(&table, "vboxdrv")),
        [TaintFlag::OutOfTreeModule, TaintFlag::UnsignedModule]
    );
}

#[test]
fn parse_keeps_a_taint_letter_it_does_not_know() {
    // Arrange: bit 19 was added for fwctl, so the alphabet demonstrably grows. Losing
    // every module on the box to report one unknown letter would be the wrong trade.
    let table = parsed("weird 4096 0 - Live 0x0000000000000000 (Z)\n");

    // Assert
    assert_eq!(
        taints_of(module(&table, "weird")),
        [TaintFlag::Unrecognised('Z')]
    );
}

#[test]
fn parse_ignores_the_going_and_coming_markers_in_the_taint_column() {
    // Arrange: `module_flags` appends `-` for a module going out and `+` for one coming
    // in, which the state column already reports.
    let table = parsed("leaving 4096 0 - Unloading 0x0000000000000000 (OE-)\n");

    // Assert
    let module = module(&table, "leaving");
    assert_eq!(
        taints_of(module),
        [TaintFlag::OutOfTreeModule, TaintFlag::UnsignedModule]
    );
    assert_eq!(module.state, ModuleState::Unloading);
}

#[test]
fn taint_flag_knows_the_whole_kernel_alphabet() {
    let cases = [
        ('P', TaintFlag::ProprietaryModule, "proprietary_module"),
        ('F', TaintFlag::ForcedModule, "forced_module"),
        (
            'S',
            TaintFlag::OutOfSpecificationSystem,
            "out_of_specification_system",
        ),
        ('R', TaintFlag::ForcedUnload, "forced_unload"),
        (
            'M',
            TaintFlag::MachineCheckException,
            "machine_check_exception",
        ),
        ('B', TaintFlag::BadPage, "bad_page"),
        ('U', TaintFlag::UserspaceRequested, "userspace_requested"),
        ('D', TaintFlag::KernelDied, "kernel_died"),
        ('A', TaintFlag::AcpiTableOverridden, "acpi_table_overridden"),
        ('W', TaintFlag::WarningIssued, "warning_issued"),
        ('C', TaintFlag::StagingDriver, "staging_driver"),
        ('I', TaintFlag::FirmwareWorkaround, "firmware_workaround"),
        ('O', TaintFlag::OutOfTreeModule, "out_of_tree_module"),
        ('E', TaintFlag::UnsignedModule, "unsigned_module"),
        ('L', TaintFlag::SoftLockup, "soft_lockup"),
        ('K', TaintFlag::LivePatched, "live_patched"),
        ('X', TaintFlag::Auxiliary, "auxiliary"),
        ('T', TaintFlag::RandstructPlugin, "randstruct_plugin"),
        ('N', TaintFlag::InKernelTest, "in_kernel_test"),
        ('J', TaintFlag::FwctlMutatingDebug, "fwctl_mutating_debug"),
    ];
    for (letter, expected, name) in cases {
        let flag = TaintFlag::from_letter(letter);
        assert_eq!(flag, expected, "wrong taint for {letter:?}");
        assert_eq!(flag.to_name(), name, "wrong rendered name for {letter:?}");
    }
}

#[test]
fn taint_flag_spells_an_unknown_letter_in_the_document() {
    let flag = TaintFlag::from_letter('Z');
    assert_eq!(flag, TaintFlag::Unrecognised('Z'));
    assert_eq!(flag.to_name(), "unrecognised_Z");
}

#[test]
fn parse_ignores_blank_lines() {
    // Act
    let table = parsed("\nnft_ct 24576 2 - Live 0x0000000000000000\n\n");

    // Assert
    assert_eq!(table.modules().len(), 1);
}

#[test]
fn parse_refuses_a_line_with_too_few_columns() {
    // Act
    let result = ProcModules::parse("nft_ct 24576 2\n");

    // Assert: a table rastro cannot parse is reported, never quietly skipped.
    assert!(result.is_err());
}

#[test]
fn parse_refuses_a_state_the_kernel_does_not_report() {
    // Act: the kernel prints exactly three, so anything else means the columns were
    // read in the wrong order.
    let result = ProcModules::parse("nft_ct 24576 2 - Zombie 0x0000000000000000\n");

    // Assert
    assert!(result.is_err());
}

#[test]
fn parse_refuses_a_size_that_is_not_a_number() {
    // Act
    let result = ProcModules::parse("nft_ct huge 2 - Live 0x0000000000000000\n");

    // Assert
    assert!(result.is_err());
}

#[test]
fn parse_refuses_a_module_reported_twice() {
    // Act: the kernel cannot produce this, so it means rastro misread the table.
    let result = ProcModules::parse(
        "nft_ct 24576 2 - Live 0x0000000000000000\nnft_ct 24576 2 - Live 0x0000000000000000\n",
    );

    // Assert: keeping the last of two would drop a loaded module from a document
    // claiming to be complete.
    assert!(result.is_err());
}

#[test]
fn the_observation_of_a_module_carries_the_contracted_keys() {
    // Arrange: the key names are the output contract, so one test reads the rendered
    // shape rather than the model.
    let table = parsed("nft_ct 24576 2 - Live 0x0000000000000000\n");

    // Act
    let observation = Observation::from(module(&table, "nft_ct"));

    // Assert
    let Content::Object(entries) = observation.content() else {
        panic!("a module renders as an object");
    };
    assert_eq!(
        entries.keys().map(String::as_str).collect::<Vec<&str>>(),
        [
            "dependants",
            "reference_count",
            "removability",
            "size_bytes",
            "state",
            "taints"
        ]
    );
    assert_eq!(
        entries["size_bytes"].content(),
        &Content::Scalar(Scalar::Integer(24576))
    );
}

#[test]
fn the_reference_count_does_not_reach_the_diffable_view() {
    // Arrange: it moves as unrelated things attach to a module, with nothing having
    // changed about the host.
    let table = parsed("nft_ct 24576 2 - Live 0x0000000000000000\n");
    let observation = Observation::from(&table);

    // Act
    let diffable = observation
        .in_view(View::Diffable)
        .expect("the table itself is not volatile");

    // Assert
    let Content::Object(modules) = diffable.content() else {
        panic!("the table renders as an object keyed by name");
    };
    let Content::Object(entries) = modules["nft_ct"].content() else {
        panic!("a module renders as an object");
    };
    assert!(
        !entries.contains_key("reference_count"),
        "got {:?}",
        entries.keys().collect::<Vec<&String>>()
    );
    assert!(entries.contains_key("size_bytes"), "the rest must survive");
}

#[test]
fn the_load_address_is_not_recorded_at_all() {
    // Arrange: it changes every boot, so it is pure noise, and it is a kernel text
    // pointer, so publishing it would hand over a KASLR offset for nothing.
    let table = parsed("nft_ct 24576 2 - Live 0xffffffffc0a12000\n");

    // Act: the complete view is the one that keeps volatile values, so if the address
    // were merely annotated rather than dropped, it would show up here.
    let complete = Observation::from(&table)
        .in_view(View::Complete)
        .expect("nothing drops the table");

    // Assert
    let rendered = format!("{complete:?}");
    assert!(
        !rendered.contains("ffffffffc0a12000"),
        "the address reached the document: {rendered}"
    );
}

/// A fixture on disk, named per test so parallel runs cannot clash.
fn fixture(name: &str, contents: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("rastro-modules-{name}"));
    std::fs::write(&path, contents).expect("the temp directory should be writable");
    path
}

/// A path that is guaranteed not to exist, for the absent and unreadable cases.
fn missing(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("rastro-modules-absent-{name}"))
}

/// A directory that looks like a mounted procfs, because it has the one entry only procfs
/// provides.
///
/// A plain directory will not do, and using one is what hid a bug: `/proc` exists as a
/// directory on a Debian box whether or not procfs is mounted, so a test that passes any
/// directory is asserting that a directory exists, not that kernel state is readable.
fn mounted_procfs(name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("rastro-procfs-{name}"));
    std::fs::create_dir_all(root.join("self")).expect("the temp directory should be writable");
    root
}

/// A directory with nothing procfs-specific in it, which is what a chroot without procfs
/// looks like.
fn unmounted_procfs(name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("rastro-no-procfs-{name}"));
    std::fs::create_dir_all(&root).expect("the temp directory should be writable");
    root
}

#[test]
fn read_translates_a_table_on_disk_into_the_model() {
    // Arrange: every other test goes through `parse`, which leaves the file reading and
    // its error path uncovered.
    let path = fixture("read", "nft_ct 24576 2 - Live 0x0000000000000000\n");
    let source = ProcModules::at(&path, "/proc");

    // Act
    let table = source.read().expect("this fixture is well formed");

    // Assert
    assert_eq!(table.modules().len(), 1);
}

#[test]
fn read_names_the_file_it_could_not_open() {
    // Arrange
    let path = missing("read");
    let source = ProcModules::at(&path, "/proc");

    // Act
    let failure = source.read().expect_err("a missing file is a failure");

    // Assert: an operator reading stderr should not have to guess which file it was.
    assert!(
        failure.to_string().contains(&path.display().to_string()),
        "got {failure}"
    );
}

#[test]
fn presence_is_present_when_the_interface_is_there() {
    // Arrange
    let path = fixture("present", "nft_ct 24576 2 - Live 0x0000000000000000\n");
    let collector = ModulesCollector::reading(ProcModules::at(&path, mounted_procfs("present")));

    // Act & Assert
    assert_eq!(collector.presence(), Presence::Present);
}

#[test]
fn presence_is_absent_when_the_kernel_has_no_module_support() {
    // Arrange: no `/proc/modules`, but procfs really is mounted. That is a
    // `CONFIG_MODULES=n` kernel, which genuinely has no modules.
    let collector =
        ModulesCollector::reading(ProcModules::at(missing("absent"), mounted_procfs("absent")));

    // Act & Assert
    assert_eq!(collector.presence(), Presence::Absent);
}

#[test]
fn presence_is_undetermined_when_procfs_is_not_mounted() {
    // Arrange: no modules file, and a `/proc` that exists as a directory with nothing
    // mounted on it, which is what a chroot looks like. Reporting "no modules" here would be
    // a confident lie, and it is the case a plain-directory check gets wrong.
    let collector = ModulesCollector::reading(ProcModules::at(
        missing("undetermined"),
        unmounted_procfs("undetermined"),
    ));

    // Act
    let presence = collector.presence();

    // Assert
    let Presence::Undetermined { reason } = presence else {
        panic!("expected undetermined, got {presence:?}");
    };
    assert!(reason.contains("not mounted"), "got {reason:?}");
}

#[test]
fn parse_reports_removability_as_unknown_when_the_kernel_cannot_unload() {
    // Arrange: `CONFIG_MODULE_UNLOAD=n` compiles out the code that prints `[permanent]`, and
    // its stub writes `-` for the count and the dependants both. So the marker being absent
    // there means the opposite of what it means on a normal kernel.
    let table = parsed("ext4 962560 - - Live 0x0000000000000000\n");

    // Assert: answering `removable` would be a confident lie about every module on the box,
    // since nothing on such a kernel can ever be unloaded.
    let module = module(&table, "ext4");
    assert_eq!(module.removability, Removability::Unknown);
    assert_eq!(module.reference_count, ReferenceCount::NotTracked);
}

#[test]
fn parse_still_reports_removable_when_the_kernel_does_track_unloading() {
    // Arrange: a counted reference means the kernel tracks unloading, so the absence of
    // `[permanent]` genuinely means removable.
    let table = parsed("nft_ct 24576 2 - Live 0x0000000000000000\n");

    // Assert
    assert_eq!(
        module(&table, "nft_ct").removability,
        Removability::Removable
    );
}
