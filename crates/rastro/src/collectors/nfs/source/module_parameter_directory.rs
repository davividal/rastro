//! `/sys/module`, where the kernel publishes each loaded module's parameters.

use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use super::kernel_file::read_value;
use crate::collectors::nfs::model::ModuleParameters;
use crate::collectors::nfs::value_objects::ParameterName;

/// Where the kernel lists loaded modules, built-in ones with parameters included.
const SYS_MODULE: &str = "/sys/module";

/// The directory of a module that holds its parameters.
const PARAMETERS: &str = "parameters";

/// The read bits for user, group and other together.
///
/// A parameter no account may read is a button, not a setting, and the mode is how the kernel
/// says so. Deciding it from a refused read instead would hide a setting an unprivileged run was
/// refused, which is state rastro failed to see.
const ANY_READ_BIT: u32 = 0o444;

/// The most parameters a module is listed to. `nfs` has twelve.
const MOST_LISTED: usize = 1_000;

/// The module directory, as a source rastro can read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleParameterDirectory {
    root: PathBuf,
}

impl ModuleParameterDirectory {
    pub fn new() -> Self {
        Self::at(SYS_MODULE)
    }

    pub fn at(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    /// Whether a module is loaded, or built in with something to publish.
    pub fn is_loaded(&self, module: &str) -> bool {
        self.root.join(module).is_dir()
    }

    /// The module's parameters, or `None` where it is not loaded.
    pub fn parameters_of(&self, module: &str) -> Option<ModuleParameters> {
        if !self.is_loaded(module) {
            return None;
        }

        let directory = self.root.join(module).join(PARAMETERS);
        Some(match names_in(&directory) {
            Ok(names) => listed(&directory, names),
            Err(reason) => ModuleParameters::Unlisted(reason),
        })
    }
}

impl Default for ModuleParameterDirectory {
    fn default() -> Self {
        Self::new()
    }
}

/// Every name in the directory, sorted; none where a loaded module has no parameters.
fn names_in(directory: &Path) -> Result<Vec<String>, String> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("could not list {}: {error}", directory.display())),
    };

    let mut names = Vec::new();
    for entry in entries {
        let entry =
            entry.map_err(|error| format!("could not list {}: {error}", directory.display()))?;
        names.push(entry.file_name().to_string_lossy().into_owned());
        if names.len() > MOST_LISTED {
            return Err(format!(
                "{} lists more than {MOST_LISTED} parameters",
                directory.display()
            ));
        }
    }
    names.sort();

    Ok(names)
}

fn listed(directory: &Path, names: Vec<String>) -> ModuleParameters {
    let mut parameters = BTreeMap::new();

    for name in names {
        let path = directory.join(&name);
        let readable = fs::symlink_metadata(&path)
            .map(|metadata| metadata.permissions().mode() & ANY_READ_BIT != 0)
            .unwrap_or(true);
        if !readable {
            continue;
        }

        match ParameterName::new(name) {
            Ok(name) => {
                parameters.insert(name, read_value(&path));
            }
            Err(error) => return ModuleParameters::Unlisted(error.to_string()),
        }
    }

    ModuleParameters::Listed(parameters)
}
