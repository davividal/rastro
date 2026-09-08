//! What a container is allowed to consume.

use rastro_collector::{ByteSize, NonEmptyText, Observation};

/// The limits the engine holds the container to, in the engine's own units.
///
/// **Every one is optional, and absent means unlimited.** docker spells "no limit" three
/// different ways — `0` for the memory and cpu figures, `null` for the process limit, `""`
/// for the cpu set — and all three become absent here. A memory limit recorded as `0` would
/// read as a container confined to no memory at all, which is the opposite of the truth.
///
/// **In the engine's units, which is what makes them recordable.** The document admits no
/// floating point, so `--cpus 1.5` could not be written as a number of CPUs. docker's own
/// unit for a fractional CPU is a whole number of billionths, so the fraction is carried
/// exactly rather than approximated.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ContainerLimits {
    pub memory: Option<ByteSize>,
    /// Memory plus swap together, which is the ceiling that actually binds. docker defaults
    /// it to twice the memory limit when only memory is given.
    pub memory_swap: Option<ByteSize>,
    /// The soft limit: not a ceiling, but what the kernel reclaims towards under pressure.
    pub memory_reservation: Option<ByteSize>,
    /// A CPU allowance in billionths of a CPU: `1500000000` is a CPU and a half.
    pub nano_cpus: Option<i64>,
    /// The relative weight under contention, which is a share rather than a ceiling.
    pub cpu_shares: Option<i64>,
    /// Which CPUs the container may run on: `0-1`, `0,3`.
    pub cpu_set: Option<NonEmptyText>,
    pub process_limit: Option<i64>,
}

impl From<&ContainerLimits> for Observation {
    fn from(limits: &ContainerLimits) -> Self {
        Observation::object([
            ("cpu_set", text(limits.cpu_set.as_ref())),
            ("cpu_shares", number(limits.cpu_shares)),
            (
                "memory_bytes",
                number(limits.memory.map(|size| size.bytes())),
            ),
            (
                "memory_reservation_bytes",
                number(limits.memory_reservation.map(|size| size.bytes())),
            ),
            (
                "memory_swap_bytes",
                number(limits.memory_swap.map(|size| size.bytes())),
            ),
            ("nano_cpus", number(limits.nano_cpus)),
            ("process_limit", number(limits.process_limit)),
        ])
    }
}

fn number(value: Option<i64>) -> Observation {
    match value {
        Some(value) => Observation::integer(value),
        None => Observation::null(),
    }
}

fn text(value: Option<&NonEmptyText>) -> Observation {
    match value {
        Some(value) => Observation::text(value.as_str()),
        None => Observation::null(),
    }
}
