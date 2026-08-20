//! The `/proc` process table.

use std::fs;
use std::path::{Path, PathBuf};

use rastro_collector::{AbsolutePath, CollectionError};

use super::{proc_cmdline, proc_status};
use crate::collectors::processes::model::{Process, ProcessTable};
use crate::collectors::processes::value_objects::{ProcessId, ProcessName, ProcessState};

/// Where the kernel publishes its processes.
const PROC: &str = "/proc";

/// The entry that proves a procfs is actually mounted rather than merely present.
const PROC_SELF: &str = "self";

/// The kernel's process table as a source rastro can read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcProcesses {
    procfs: PathBuf,
}

impl ProcProcesses {
    pub fn new() -> Self {
        Self {
            procfs: PathBuf::from(PROC),
        }
    }

    pub fn at(procfs: impl Into<PathBuf>) -> Self {
        Self {
            procfs: procfs.into(),
        }
    }

    pub fn filesystem(&self) -> &Path {
        &self.procfs
    }

    pub fn filesystem_is_mounted(&self) -> bool {
        self.procfs.join(PROC_SELF).exists()
    }

    /// Walks the process directories and translates each one.
    ///
    /// # A process that exits mid-walk is skipped, not an error
    ///
    /// **This is the one rule that makes the collector work at all**, and it is a departure
    /// from how every other source here treats a failed read. Listing `/proc` and then
    /// reading each entry is inherently racy: a process that was there for the listing can
    /// be gone before its `status` file is opened, and on a busy box that happens often.
    /// The disappearance is not a failure to collect and not a misread — the process really
    /// did exit — so the entry is dropped and the walk goes on.
    ///
    /// The distinction rastro can draw, and does, is *why* the read failed. A file that is
    /// no longer there means the process left. A file that is there and will not parse means
    /// the interface is not what rastro believes, and that is still a loud failure.
    pub fn read(&self) -> Result<ProcessTable, CollectionError> {
        let mut processes = Vec::new();

        for process_id in self.process_ids()? {
            if let Some(process) = self.read_one(&process_id)? {
                processes.push(process);
            }
        }

        Ok(ProcessTable::new(processes))
    }

    /// Every numeric entry under the procfs root, sorted.
    ///
    /// Sorted so that a failure names the same process on two runs over the same tree;
    /// the model sorts the result properly afterwards.
    fn process_ids(&self) -> Result<Vec<ProcessId>, CollectionError> {
        let mut found = Vec::new();

        for entry in fs::read_dir(&self.procfs).map_err(|error| {
            CollectionError::new(format!("could not list {}: {error}", self.procfs.display()))
        })? {
            let entry = entry.map_err(|error| {
                CollectionError::new(format!(
                    "could not list an entry of {}: {error}",
                    self.procfs.display()
                ))
            })?;

            let name = entry.file_name().to_string_lossy().into_owned();
            // Only the numeric entries are processes. `/proc` is full of others, and a
            // filter on "is it a directory" would sweep in `net`, `sys` and `self`.
            if name.bytes().all(|byte| byte.is_ascii_digit()) && !name.is_empty() {
                found.push(ProcessId::parse(&name)?);
            }
        }
        found.sort();

        Ok(found)
    }

    /// One process, or nothing if it left before rastro reached it.
    fn read_one(&self, process_id: &ProcessId) -> Result<Option<Process>, CollectionError> {
        let directory = self.procfs.join(process_id.as_u32().to_string());

        let Some(status) = self.read_optional(&directory.join("status"))? else {
            return Ok(None);
        };
        let Some(cmdline) = self.read_optional(&directory.join("cmdline"))? else {
            return Ok(None);
        };
        // A host with no cgroup hierarchy has no such file at all, which is different from
        // the process having gone, so an absent file becomes an absent field rather than a
        // skipped process.
        let cgroup = self.read_optional(&directory.join("cgroup"))?;

        let fields = proc_status::parse(&status);
        let process = Process {
            name: ProcessName::new(proc_status::field(&fields, proc_status::NAME)?)?,
            command_line: proc_cmdline::parse_arguments(&cmdline),
            user_id: proc_status::real_id(proc_status::field(&fields, proc_status::USER)?)?,
            group_id: proc_status::real_id(proc_status::field(&fields, proc_status::GROUP)?)?,
            control_group: match cgroup {
                Some(cgroup) => proc_cmdline::parse_control_group(&cgroup)?,
                None => None,
            },
            executable: self.read_executable(&directory)?,
            process_id: *process_id,
            parent_process_id: ProcessId::parse(proc_status::field(&fields, proc_status::PARENT)?)?,
            state: ProcessState::new(proc_status::field(&fields, proc_status::STATE)?)?,
            thread_count: proc_status::field(&fields, proc_status::THREADS)?
                .parse::<i64>()
                .map_err(|_| CollectionError::new("a thread count is not a number"))?,
        };

        Ok(Some(process))
    }

    /// The binary behind a process, if rastro can see it.
    ///
    /// Absent for a kernel thread, which has no binary, and for a process whose executable
    /// has been deleted or replaced since it started — in which case the kernel appends
    /// ` (deleted)` to the link target, which is why the value is refused rather than
    /// recorded when it is not an absolute path.
    fn read_executable(&self, directory: &Path) -> Result<Option<AbsolutePath>, CollectionError> {
        let Ok(target) = fs::read_link(directory.join("exe")) else {
            return Ok(None);
        };

        let target = target.to_string_lossy().into_owned();
        Ok(AbsolutePath::new(target, "executable path").ok())
    }

    /// Reads a file, telling "the process left" apart from "the read failed".
    fn read_optional(&self, path: &Path) -> Result<Option<String>, CollectionError> {
        match fs::read_to_string(path) {
            Ok(contents) => Ok(Some(contents)),
            // Every errno a departed process produces. `ESRCH` reaches userspace as
            // `NotFound` here, and reading a dead process's files gives `ENOENT`.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(CollectionError::new(format!(
                "could not read {}: {error}",
                path.display()
            ))),
        }
    }
}

impl Default for ProcProcesses {
    fn default() -> Self {
        Self::new()
    }
}
