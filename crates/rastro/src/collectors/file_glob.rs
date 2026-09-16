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
pub fn is_pattern(path: &Path) -> bool {
    path.components().any(|component| match component {
        Component::Normal(name) => holds_wildcard(&name.to_string_lossy()),
        _ => false,
    })
}

/// Every existing path the pattern names, in byte order.
///
/// A bracket expression is refused rather than guessed at: matching it wrongly would report
/// a set of vhosts the server does not have, and there is no way for a reader to tell that
/// from a set it does.
pub fn matching(pattern: &Path) -> Result<Vec<PathBuf>, CollectionError> {
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
            true => expanded(&found, &name),
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
/// one unreadable directory into a missing set of vhosts elsewhere.
fn expanded(candidates: &[PathBuf], pattern: &str) -> Vec<PathBuf> {
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
            let name = entry.file_name();
            if matches(&name, pattern) {
                found.push(candidate.join(&name));
            }
        }
    }

    found
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
fn wildcard_matches(name: &[char], pattern: &[char]) -> bool {
    let Some((first, rest)) = pattern.split_first() else {
        return name.is_empty();
    };

    match first {
        '*' => (0..=name.len()).any(|taken| wildcard_matches(&name[taken..], rest)),
        '?' => !name.is_empty() && wildcard_matches(&name[1..], rest),
        expected => name
            .split_first()
            .is_some_and(|(actual, tail)| actual == expected && wildcard_matches(tail, rest)),
    }
}
