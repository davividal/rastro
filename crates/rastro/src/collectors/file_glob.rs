//! The files a pattern names, resolved the way `glob(3)` resolves it.
//!
//! Shared, because two collectors meet the same problem: nginx hands an `include` argument
//! to `glob(3)` when it holds a wildcard and to `open(2)` when it does not, and systemd does
//! the same with `EnvironmentFile=`. In both the two cases fail differently — a pattern that
//! matches nothing is an ordinary empty result, while a literal path that is not there is a
//! configuration error the service itself reports. Only the pattern case lives here; what a
//! caller does with an empty result is the caller's own rule.
//!
//! Beside [`canonical_tool`](super::canonical_tool) rather than inside either collector, for
//! the same reason: one place to be right about a fiddly host interface, rather than one per
//! caller that can drift.
//!
//! **Sorted by bytes, and that is a choice rather than a copy.** `glob(3)` sorts with the
//! caller's collation, so the same directory can order differently under two locales. rastro
//! sorts by bytes so that a fingerprint means the same on every box, which differs from
//! nginx's own order only where two files differ solely in case or punctuation *and* both
//! set the same directive.

use std::ffi::OsStr;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path, PathBuf};

use rastro_collector::CollectionError;

/// The characters that make an argument a pattern rather than a path.
const WILDCARDS: [char; 2] = ['*', '?'];

/// A bracket expression, which `glob(3)` understands and this does not.
const CLASS: char = '[';

/// Whether this argument would be globbed rather than opened.
///
/// **A bracket expression counts, even though [`matching`] then refuses it.** `glob(3)` treats
/// `[ab].env` as a pattern, so a caller that read it as a literal path would report a file
/// that cannot exist as merely absent — the same silent wrongness as not expanding `*`, and
/// harder to spot because the path looks ordinary. Answering `true` here routes it to the
/// refusal instead, which is the honest answer.
pub fn is_pattern(path: &Path) -> bool {
    path.components().any(|component| match component {
        Component::Normal(name) => {
            let name = name.to_string_lossy();
            holds_wildcard(&name) || name.contains(CLASS)
        }
        _ => false,
    })
}

/// Every existing path the pattern names, in byte order.
///
/// A bracket expression is refused rather than guessed at: matching it wrongly would report
/// a set of vhosts the server does not have, and there is no way for a reader to tell that
/// from a set it does.
pub fn matching(pattern: &Path) -> Result<Vec<PathBuf>, CollectionError> {
    matching_at_most(pattern, usize::MAX)
}

/// [`matching`], refused once the directories it lists hold more than `most_entries` between them.
///
/// For a pattern from a file another account owns, a redis `include` among them: the directories
/// are that account's to fill, and every entry is listed before any budget on matches applies.
pub fn matching_at_most(
    pattern: &Path,
    most_entries: usize,
) -> Result<Vec<PathBuf>, CollectionError> {
    let mut entries_left = most_entries;
    let mut found = vec![PathBuf::new()];

    for component in pattern.components() {
        let Component::Normal(name) = component else {
            found = found
                .into_iter()
                .map(|candidate| candidate.join(component.as_os_str()))
                .collect();
            continue;
        };

        let name = name.to_string_lossy().into_owned();
        if name.contains(CLASS) {
            return Err(CollectionError::new(format!(
                "the pattern {} holds a bracket expression, which rastro does not resolve; \
                 the services that read these patterns do, so this facet would otherwise \
                 report a set of files the box does not read",
                pattern.display()
            )));
        }

        found = match holds_wildcard(&name) {
            true => expanded(&found, &name, &mut entries_left).ok_or_else(|| {
                CollectionError::new(format!(
                    "the pattern {} lists directories holding more than {most_entries} entries \
                     between them, which is wider than any configuration rastro reads them for",
                    pattern.display()
                ))
            })?,
            false => found
                .into_iter()
                .map(|candidate| candidate.join(&name))
                .collect(),
        };
    }

    found.retain(|candidate| candidate.exists());
    found.sort_by(|left, right| {
        left.as_os_str()
            .as_bytes()
            .cmp(right.as_os_str().as_bytes())
    });
    Ok(found)
}

fn holds_wildcard(name: &str) -> bool {
    name.contains(WILDCARDS)
}

/// The entries of each candidate directory whose name the pattern matches.
///
/// A directory that cannot be read contributes nothing rather than failing the pattern, the
/// same way `glob(3)` skips what it cannot open: a fingerprint run has no business turning
/// one unreadable directory into a missing set of vhosts elsewhere. Nothing at all where the
/// listing runs past `entries_left`.
fn expanded(
    candidates: &[PathBuf],
    pattern: &str,
    entries_left: &mut usize,
) -> Option<Vec<PathBuf>> {
    let mut found = Vec::new();

    for candidate in candidates {
        let directory = match candidate.as_os_str().is_empty() {
            true => Path::new("."),
            false => candidate.as_path(),
        };

        let Ok(entries) = fs::read_dir(directory) else {
            continue;
        };

        for entry in entries.flatten() {
            *entries_left = entries_left.checked_sub(1)?;
            let name = entry.file_name();
            if matches(&name, pattern) {
                found.push(candidate.join(&name));
            }
        }
    }

    Some(found)
}

/// Whether one directory entry's name matches the pattern.
///
/// A leading dot is matched only by a pattern that spells one, which is `glob(3)`'s rule and
/// the reason `include conf.d/*.conf` does not pick up an editor's `.site.conf.swp`.
fn matches(name: &OsStr, pattern: &str) -> bool {
    let name = name.to_string_lossy();

    if name.starts_with('.') != pattern.starts_with('.') {
        return false;
    }

    wildcard_matches(
        &name.chars().collect::<Vec<char>>(),
        &pattern.chars().collect::<Vec<char>>(),
    )
}

/// `*` for any run of characters, `?` for exactly one, everything else itself.
///
/// **Iterative, in time the product of the two lengths at worst.** Trying every split for every
/// star is exponential in the stars, and a pattern can come from a file another account owns, a
/// redis `include` among them. A mismatch only ever resumes from the last star, one character
/// further on, which is enough because a later star can absorb anything an earlier one could.
fn wildcard_matches(name: &[char], pattern: &[char]) -> bool {
    let (mut at_name, mut at_pattern) = (0, 0);
    let mut last_star: Option<(usize, usize)> = None;

    while at_name < name.len() {
        match pattern.get(at_pattern) {
            Some('*') => {
                last_star = Some((at_pattern, at_name));
                at_pattern += 1;
            }
            Some('?') => {
                at_name += 1;
                at_pattern += 1;
            }
            Some(expected) if *expected == name[at_name] => {
                at_name += 1;
                at_pattern += 1;
            }
            _ => match last_star {
                Some((star, absorbed_from)) => {
                    at_pattern = star + 1;
                    at_name = absorbed_from + 1;
                    last_star = Some((star, absorbed_from + 1));
                }
                None => return false,
            },
        }
    }

    pattern[at_pattern..]
        .iter()
        .all(|remaining| *remaining == '*')
}
