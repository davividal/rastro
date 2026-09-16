//! Finding a running process by the binary behind it, and whose it is.

use std::ffi::OsStr;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

/// One running process rastro found, with the account that owns it.
///
/// **The owner comes from the process's own `/proc` directory**, whose uid the kernel sets
/// to the process's real uid. That is one `stat` rather than a parse of `status`, and it is
/// what tells a rootless engine from the system one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunningProcess {
    pub user_id: u32,
    pub arguments: Vec<String>,
}

/// Every running process whose executable has this name.
///
/// **Identified by the binary rather than by a name**, for the reason the `exporters` facet
/// gives about units: a process can be called anything, and `/proc/<pid>/exe` is the fact.
/// Shared by the two dialects that have to find an engine's process before they can ask it
/// anything — containerd, to learn which socket it is listening on, and podman, to learn
/// which services are running and whose they are.
///
/// A process that vanishes between being listed and being read is skipped, which is the
/// same race every `/proc` walk has and the same treatment the `processes` facet gives it.
pub fn running(proc: impl AsRef<Path>, program: &str) -> Vec<RunningProcess> {
    let Ok(entries) = fs::read_dir(proc.as_ref()) else {
        return Vec::new();
    };

    let mut found = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        let is_process = path
            .file_name()
            .and_then(OsStr::to_str)
            .is_some_and(|name| name.chars().all(|character| character.is_ascii_digit()));
        if !is_process {
            continue;
        }

        let Ok(executable) = fs::read_link(path.join("exe")) else {
            continue;
        };
        if executable.file_name().and_then(OsStr::to_str) != Some(program) {
            continue;
        }

        let (Ok(raw), Ok(owner)) = (fs::read(path.join("cmdline")), fs::metadata(&path)) else {
            continue;
        };

        found.push(RunningProcess {
            user_id: owner.uid(),
            arguments: String::from_utf8_lossy(&raw)
                .split('\0')
                .filter(|argument| !argument.is_empty())
                .map(str::to_owned)
                .collect(),
        });
    }

    found
}
