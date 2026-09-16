//! Reading PAM's two default environment sources off the host.

use std::fs;
use std::io;

use rastro_collector::AbsolutePath;

use super::{environment_file, pam_env_conf};
use crate::collectors::pam::model::{FileStatus, RulesFile, SessionEnvironment, VariablesFile};

/// Read by `pam_env` with its default `readenv=1`.
const ENVIRONMENT: &str = "/etc/environment";

/// `pam_env`'s own configuration.
const PAM_ENV_CONF: &str = "/etc/security/pam_env.conf";

/// Two orders of magnitude above either file on a real box, and still bounded.
///
/// Neither is large by construction — PAM reads both at every login — so one past this is a
/// misconfiguration. The bound is here for the same reason the unit environment files have
/// one: the paths are fixed rather than operator-supplied, but a fingerprint that cannot
/// finish is worse than one that records a refusal.
const LARGEST: u64 = 1024 * 1024;

/// What the box's PAM configuration declares for a session's environment.
pub fn read() -> SessionEnvironment {
    SessionEnvironment {
        variables: variables_from(ENVIRONMENT),
        rules: rules_from(PAM_ENV_CONF),
    }
}

fn variables_from(path: &str) -> VariablesFile {
    let recorded =
        AbsolutePath::new(path, "pam environment file").expect("the compiled-in path is absolute");

    match text_of(path) {
        Ok(text) => {
            let parsed = environment_file::parse(&text);

            VariablesFile {
                path: recorded,
                status: FileStatus::Ok,
                variables: parsed.variables,
                ignored_lines: parsed.ignored_lines,
            }
        }
        Err(status) => VariablesFile {
            path: recorded,
            status,
            variables: Default::default(),
            ignored_lines: 0,
        },
    }
}

fn rules_from(path: &str) -> RulesFile {
    let recorded =
        AbsolutePath::new(path, "pam_env configuration").expect("the compiled-in path is absolute");

    match text_of(path) {
        Ok(text) => {
            let parsed = pam_env_conf::parse(&text);

            RulesFile {
                path: recorded,
                status: FileStatus::Ok,
                rules: parsed.rules,
                ignored_lines: parsed.ignored_lines,
            }
        }
        Err(status) => RulesFile {
            path: recorded,
            status,
            rules: Vec::new(),
            ignored_lines: 0,
        },
    }
}

/// The file's text, or the status that stands in its place.
///
/// Stat before open and a bounded read, the same discipline the unit environment files get:
/// these paths are fixed rather than operator-supplied, so a FIFO here would be somebody
/// having replaced a system file — which is exactly the sort of thing a fingerprint should
/// record rather than hang on.
fn text_of(path: &str) -> Result<String, FileStatus> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Err(FileStatus::Absent),
        Err(error) => return Err(FileStatus::Unreadable(error.to_string())),
    };

    if !metadata.is_file() {
        return Err(FileStatus::Unreadable(format!(
            "{path} is not a regular file, and rastro will not read one that could block or \
             never end"
        )));
    }

    if metadata.len() > LARGEST {
        return Err(FileStatus::Unreadable(format!(
            "{path} is {} bytes, past the {LARGEST} rastro reads of a PAM environment source",
            metadata.len()
        )));
    }

    fs::read_to_string(path).map_err(|error| FileStatus::Unreadable(error.to_string()))
}
