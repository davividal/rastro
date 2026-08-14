//! What is mounted where, and how.

use std::fs;

// One import, because `rastro-collector` re-exports what an author needs. A
// collector written outside this repo looks exactly like this.
use rastro_collector::{
    CollectionError, Collector, CollectorCategory, CollectorId, CollectorIdentity,
    CollectorVersion, FacetName, Observation, Presence,
};

const MOUNTS_PATH: &str = "/proc/mounts";

pub struct MountsCollector {
    name: FacetName,
    identity: CollectorIdentity,
}

impl MountsCollector {
    pub fn new() -> Self {
        Self {
            name: FacetName::new("mounts").expect("`mounts` is a legal facet name"),
            identity: CollectorIdentity::new(
                CollectorId::new("mounts").expect("`mounts` is a legal collector id"),
                CollectorVersion::new("1").expect("`1` is a legal collector version"),
            ),
        }
    }
}

impl Default for MountsCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl Collector for MountsCollector {
    fn name(&self) -> &FacetName {
        &self.name
    }

    fn identity(&self) -> &CollectorIdentity {
        &self.identity
    }

    fn category(&self) -> CollectorCategory {
        CollectorCategory::State
    }

    /// Always present: a running host has mounts, whatever they turn out to be.
    ///
    /// An unreadable table is a failure to read them, never evidence that there
    /// are none, so it surfaces from `collect` as an error rather than as a
    /// confident and wrong `absent`.
    fn presence(&self) -> Presence {
        Presence::Present
    }

    fn collect(&self) -> Result<Observation, CollectionError> {
        let table = fs::read_to_string(MOUNTS_PATH).map_err(|error| {
            CollectionError::new(format!("could not read {MOUNTS_PATH}: {error}"))
        })?;

        parse_mount_table(&table)
    }
}

/// Parses the `/proc/mounts` table.
///
/// A list rather than a map keyed by mount point, because a mount point can
/// legitimately appear twice: stacked and bind mounts are real, and keying
/// would drop one of them silently.
///
/// Kernel order is kept for the same reason. It is stable between two runs of
/// an unchanged host, so it satisfies the ordering rule, and it carries the
/// stacking that a sort would discard. Options *are* sorted, since they are a
/// set and their order is arbitrary churn.
pub fn parse_mount_table(table: &str) -> Result<Observation, CollectionError> {
    let mut mounts = Vec::new();

    for line in table.lines().filter(|line| !line.trim().is_empty()) {
        let fields: Vec<&str> = line.split_whitespace().collect();
        let [device, mount_point, filesystem, options, ..] = fields.as_slice() else {
            return Err(CollectionError::new(format!(
                "malformed line in {MOUNTS_PATH}: {line:?}"
            )));
        };

        // Decoded before sorting, not after: sorting the escaped text would
        // order options by an artefact of the transport encoding. `a\040b`
        // sorts after `aB` escaped and before it decoded.
        let mut decoded_options: Vec<String> =
            split_options(options).into_iter().map(unescape).collect();
        decoded_options.sort_unstable();

        mounts.push(Observation::object([
            ("device", Observation::text(unescape(device))),
            ("mount_point", Observation::text(unescape(mount_point))),
            ("filesystem", Observation::text(unescape(filesystem))),
            (
                "options",
                Observation::list(decoded_options.into_iter().map(Observation::text)),
            ),
        ]));
    }

    Ok(Observation::list(mounts))
}

/// The four sequences the kernel writes, and nothing else.
///
/// `fs/proc_namespace.c` escapes exactly `" \t\n\\"`, so this is the whole set.
/// Decoding any three-digit octal escape would be wrong: values above 127 are
/// raw bytes of a UTF-8 sequence, not characters, and the kernel never emits
/// them anyway.
const KERNEL_ESCAPES: [(&str, char); 4] = [
    ("\\040", ' '),
    ("\\011", '\t'),
    ("\\012", '\n'),
    ("\\134", '\\'),
];

/// Undoes the kernel's escaping of whitespace in a field.
///
/// The escape exists so the table stays safe to tokenise on whitespace, which
/// is why `split_whitespace` above is correct. It is a transport encoding, not
/// the state of the host: a filesystem mounted at `/mnt/My Drive` is recorded
/// under that name, not under `/mnt/My\040Drive`.
///
/// A backslash that does not begin one of the four sequences is kept as it is.
fn unescape(field: &str) -> String {
    let mut decoded = String::with_capacity(field.len());
    let mut rest = field;

    while let Some(backslash) = rest.find('\\') {
        decoded.push_str(&rest[..backslash]);
        let candidate = &rest[backslash..];

        match KERNEL_ESCAPES
            .iter()
            .find(|(escape, _)| candidate.starts_with(escape))
        {
            Some((escape, character)) => {
                decoded.push(*character);
                rest = &candidate[escape.len()..];
            }
            None => {
                decoded.push('\\');
                rest = &candidate[1..];
            }
        }
    }
    decoded.push_str(rest);

    decoded
}

/// Splits an option list on the commas that separate options, not on the ones
/// inside a quoted value.
///
/// SELinux writes `context="system_u:object_r:container_file_t:s0:c132,c369"`,
/// and splitting that naively invents two options that were never mounted.
fn split_options(options: &str) -> Vec<&str> {
    let mut split = Vec::new();
    let mut option_start = 0;
    let mut inside_quotes = false;

    for (index, character) in options.char_indices() {
        match character {
            '"' => inside_quotes = !inside_quotes,
            ',' if !inside_quotes => {
                split.push(&options[option_start..index]);
                option_start = index + 1;
            }
            _ => {}
        }
    }
    split.push(&options[option_start..]);

    split
}
