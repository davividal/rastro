//! The `/proc/<pid>/cmdline` and `/proc/<pid>/cgroup` interfaces.

use rastro_collector::CollectionError;

use crate::collectors::processes::value_objects::{CommandLine, ControlGroup};

/// What separates the arguments in `cmdline`.
const ARGUMENT_SEPARATOR: char = '\0';

/// What cgroup v2 prefixes its single line with.
///
/// The file is `hierarchy-id:controllers:path`. Under the unified hierarchy the first two
/// are always empty, giving `0::/system.slice/ssh.service`.
const UNIFIED_PREFIX: &str = "0::";

/// Splits `cmdline` into arguments.
///
/// **The trailing NUL is dropped, and dropping exactly one is the point.** The kernel
/// terminates every argument including the last, so a naive split yields a spurious empty
/// argument at the end. Trimming *all* trailing empties instead would delete a genuine
/// empty final argument, which a program invoked with `""` really has.
pub fn parse_arguments(cmdline: &str) -> CommandLine {
    let Some(body) = cmdline.strip_suffix(ARGUMENT_SEPARATOR) else {
        // No terminator at all: either empty, which is a kernel thread, or a truncated
        // read. Split what is there rather than inventing an argument.
        return CommandLine::new(
            cmdline
                .split(ARGUMENT_SEPARATOR)
                .filter(|argument| !argument.is_empty())
                .map(str::to_owned),
        );
    };

    CommandLine::new(body.split(ARGUMENT_SEPARATOR).map(str::to_owned))
}

/// Reads the control group out of the `cgroup` file.
///
/// Only the unified hierarchy's line is read. A host still on cgroup v1 writes one line per
/// controller with no single answer to give, so rather than picking one arbitrarily the
/// field is reported absent. Debian 12 has used the unified hierarchy since it shipped, and
/// `systemctl --version` confirms it with `default-hierarchy=unified`.
pub fn parse_control_group(cgroup: &str) -> Result<Option<ControlGroup>, CollectionError> {
    let Some(line) = cgroup
        .lines()
        .find_map(|line| line.strip_prefix(UNIFIED_PREFIX))
    else {
        return Ok(None);
    };

    if line.trim().is_empty() {
        return Ok(None);
    }

    Ok(Some(ControlGroup::new(line.trim())?))
}
