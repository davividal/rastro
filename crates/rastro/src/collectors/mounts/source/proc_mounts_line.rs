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

/// The length of a `\NNN` sequence, backslash included.
const ESCAPE_LENGTH: usize = 4;

/// The first value the kernel does not escape this way.
///
/// Above this a byte is part of a UTF-8 sequence rather than a character of its own, and
/// reassembling those would need byte-level handling this does not do. The kernel does not emit
/// them for these columns.
const HIGHEST_ESCAPED: u32 = 0o200;

/// Undoes the kernel's octal escaping of a column.
///
/// This decodes the *encoding*, not a list of the kernel's call sites, and the difference
/// matters because there are four of them and they do not agree on the alphabet:
///
/// | written by | escapes |
/// | --- | --- |
/// | `seq_path_root`, for the mount point | `" \t\n\\"` |
/// | `mangle`, for the device and filesystem type | `" \t\n\\#"` |
/// | `seq_show_option`, for an option name | `",= \t\n\\"` |
/// | `seq_show_option`, for an option value | `", \t\n\\"` |
/// | `show_sid` in SELinux, for a context | `"\"\n\\"` |
///
/// A table of those sets was wrong twice on this branch alone, first missing `\043` and then
/// `\054`, `\075` and `\042`. So the rule is the encoding itself: a backslash and three octal
/// digits below [`HIGHEST_ESCAPED`]. A new escaping call site then costs nothing.
///
/// **What makes one rule safe for every column:** every one of those paths escapes the backslash
/// itself, as `\134`. A bare `\NNN` therefore cannot occur in any column except as a genuine
/// escape, so decoding sequences a particular column never emits cannot corrupt it.
///
/// The escape is a transport encoding, not the state of the host: a filesystem mounted at
/// `/mnt/My Drive` is recorded under that name, not under `/mnt/My\040Drive`.
fn unescape(column: &str) -> String {
    let mut decoded = String::with_capacity(column.len());
    let mut rest = column;

    while let Some(backslash) = rest.find('\\') {
        decoded.push_str(&rest[..backslash]);
        let candidate = &rest[backslash..];

        match octal_escape(candidate) {
            Some(character) => {
                decoded.push(character);
                rest = &candidate[ESCAPE_LENGTH..];
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

/// The character a `\NNN` sequence stands for, if the candidate is one.
///
/// All three digits are checked against `0` to `7` by hand rather than left to
/// `from_str_radix`, which accepts a leading sign and would read `\+12` as an escape.
fn octal_escape(candidate: &str) -> Option<char> {
    let digits = candidate.get(1..ESCAPE_LENGTH)?;
    if !digits.bytes().all(|digit| (b'0'..=b'7').contains(&digit)) {
        return None;
    }

    let value = u32::from_str_radix(digits, 8).ok()?;
    if value >= HIGHEST_ESCAPED {
        return None;
    }

    char::from_u32(value)
}

/// Splits the option column on the commas that separate options, not on the ones inside a
/// quoted value.
///
/// SELinux writes `context="system_u:object_r:container_file_t:s0:c132,c369"`, and splitting
/// that naively invents two options that were never mounted.
///
/// A quote only opens a quoted region when it directly follows `=`, which is how `show_sid`
/// writes the one form this exists for. Toggling on *any* quote was a real defect: a quote is a
/// legal path character that `seq_show_option` does not escape, so an overlay whose lower
/// directory contained one desynchronised the rest of the line and silently fused every option
/// after it into a single bogus value.
fn split_options(options: &str) -> Vec<&str> {
    let mut split = Vec::new();
    let mut option_start = 0;
    let mut inside_quotes = false;
    let mut previous = char::default();

    for (index, character) in options.char_indices() {
        match character {
            '"' if inside_quotes => inside_quotes = false,
            '"' if previous == '=' => inside_quotes = true,
            ',' if !inside_quotes => {
                split.push(&options[option_start..index]);
                option_start = index + 1;
            }
            _ => {}
        }

        previous = character;
    }
    split.push(&options[option_start..]);

    split
}
