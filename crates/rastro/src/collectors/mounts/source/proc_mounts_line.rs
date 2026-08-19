//! One line of `/proc/mounts`, as the kernel wrote it.
//!
//! The kernel's spelling, kept apart from rastro's meaning. Everything peculiar to
//! this interface lives here: six positional columns, whitespace escaped into octal
//! sequences, and an option list that is one comma-joined string.

use rastro_collector::CollectionError;

use crate::collectors::mounts::model::Mount;
use crate::collectors::mounts::value_objects::{
    Device, FilesystemType, MountOption, MountOptions, MountPoint,
};

/// The columns of one line that rastro reads.
///
/// The kernel writes six. The last two are `dump` frequency and `fsck` order, a
/// constant `0 0` for every mount, so they are dropped at this boundary rather than
/// carried into the model. Dropping them is still not the same as ignoring them:
/// [`Self::parse`] insists on all six, where a `..` slice pattern would silently
/// accept a truncated line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcMountsLine {
    device: String,
    mount_point: String,
    filesystem_type: String,
    options: String,
}

impl ProcMountsLine {
    /// Splits one line into its columns, on the separator the kernel actually writes.
    ///
    /// A single space, not "whitespace". The kernel escapes exactly space, tab, newline and
    /// backslash, so every *other* whitespace character reaches this function unescaped and
    /// inside a value: U+00A0 from a Windows share name, a bare carriage return, a vertical
    /// tab, U+3000. `split_whitespace` splits on the full Unicode `White_Space` set and would
    /// find a seventh column, failing the exact-six check and losing the whole mount table to
    /// one oddly named directory.
    ///
    /// Reporting a table rastro half-understood as complete is the one failure this project
    /// will not accept, so a line that is not what this interface promises is still refused
    /// rather than skipped.
    pub fn parse(line: &str) -> Result<Self, CollectionError> {
        let columns: Vec<&str> = line
            .split(' ')
            .filter(|column| !column.is_empty())
            .collect();
        let [
            device,
            mount_point,
            filesystem_type,
            options,
            _dump_frequency,
            _check_order,
        ] = columns.as_slice()
        else {
            return Err(CollectionError::new(format!(
                "expected six columns in a /proc/mounts line, got {}: {line:?}",
                columns.len()
            )));
        };

        Ok(Self {
            device: (*device).to_owned(),
            mount_point: (*mount_point).to_owned(),
            filesystem_type: (*filesystem_type).to_owned(),
            options: (*options).to_owned(),
        })
    }

    /// Translates the kernel's spelling into rastro's model.
    ///
    /// Decoding happens here rather than in the values, so that a second interface
    /// reporting the same concepts differently needs no change to the model.
    pub fn to_mount(&self) -> Result<Mount, CollectionError> {
        Ok(Mount {
            device: Device::new(unescape(&self.device))?,
            mount_point: MountPoint::new(unescape(&self.mount_point))?,
            filesystem: FilesystemType::new(unescape(&self.filesystem_type))?,
            options: self.to_mount_options()?,
        })
    }

    fn to_mount_options(&self) -> Result<MountOptions, CollectionError> {
        let options = split_options(&self.options)
            .into_iter()
            .map(|option| MountOption::new(unescape(option)))
            .collect::<Result<Vec<MountOption>, CollectionError>>()?;

        Ok(MountOptions::new(options))
    }
}

/// The four sequences the kernel writes, and nothing else.
///
/// `fs/proc_namespace.c` escapes exactly `" \t\n\\"`, so this is the whole set.
/// Decoding any three-digit octal escape would be wrong: values above 127 are raw
/// bytes of a UTF-8 sequence, not characters, and the kernel never emits them
/// anyway.
const KERNEL_ESCAPES: [(&str, char); 4] = [
    ("\\040", ' '),
    ("\\011", '\t'),
    ("\\012", '\n'),
    ("\\134", '\\'),
];

/// Undoes the kernel's escaping of whitespace in a column.
///
/// The escape is a transport encoding, not the state of the host: a filesystem
/// mounted at `/mnt/My Drive` is recorded under that name, not under
/// `/mnt/My\040Drive`.
///
/// A backslash that does not begin one of the four sequences is kept as it is.
fn unescape(column: &str) -> String {
    let mut decoded = String::with_capacity(column.len());
    let mut rest = column;

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

/// Splits the option column on the commas that separate options, not on the ones
/// inside a quoted value.
///
/// SELinux writes `context="system_u:object_r:container_file_t:s0:c132,c369"`, and
/// splitting that naively invents two options that were never mounted.
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
