//! Finding a running process by the binary behind it.

use std::fs;
use std::path::Path;

/// The command lines of every running process whose executable has this name.
///
/// **Identified by the binary rather than by a name**, for the reason the `exporters` facet
/// gives about units: a process can be called anything, and `/proc/<pid>/exe` is the fact.
/// Shared by the two dialects that have to find an engine's process before they can ask it
/// anything — containerd, to learn which socket it is listening on, and podman, to learn
/// whether a service is running at all.
///
/// A process that vanishes between being listed and being read is skipped, which is the
/// same race every `/proc` walk has and the same treatment the `processes` facet gives it.
pub fn command_lines_of(proc: impl AsRef<Path>, program: &str) -> Vec<Vec<String>> {
    let Ok(entries) = fs::read_dir(proc.as_ref()) else {
        return Vec::new();
    };

    let mut found = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        let is_process = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.chars().all(|character| character.is_ascii_digit()));
        if !is_process {
            continue;
        }

        let Ok(executable) = fs::read_link(path.join("exe")) else {
            continue;
        };
        if executable.file_name().and_then(|name| name.to_str()) != Some(program) {
            continue;
        }

        let Ok(raw) = fs::read(path.join("cmdline")) else {
            continue;
        };

        found.push(
            String::from_utf8_lossy(&raw)
                .split('\0')
                .filter(|argument| !argument.is_empty())
                .map(str::to_owned)
                .collect(),
        );
    }

    found
}
