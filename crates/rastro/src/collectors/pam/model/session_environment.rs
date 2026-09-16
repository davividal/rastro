//! The two sources `pam_env` reads by default, and what each held.

use std::collections::BTreeMap;

use rastro_collector::{AbsolutePath, EnvironmentVariableName, Observation};

use super::environment_rule::EnvironmentRule;
use super::file_status::FileStatus;

/// PAM's session environment, as its two default sources declare it.
///
/// **Two sources and not one map, because they are different kinds of statement.**
/// `/etc/environment` assigns values outright; `pam_env.conf` carries rules that apply
/// conditionally and may expand per account. Merging them would need rastro to decide which
/// wins for an account it is not logging in as, which is a resolution it cannot honestly
/// perform.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionEnvironment {
    pub variables: VariablesFile,
    pub rules: RulesFile,
}

/// `/etc/environment`: plain assignments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariablesFile {
    pub path: AbsolutePath,
    pub status: FileStatus,
    /// Names in the clear, values withheld — the split every environment carrier here makes.
    pub variables: BTreeMap<EnvironmentVariableName, String>,
    pub ignored_lines: usize,
}

/// `/etc/security/pam_env.conf`: rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RulesFile {
    pub path: AbsolutePath,
    pub status: FileStatus,
    /// In the file's order, which is the order `pam_env` applies them in.
    pub rules: Vec<EnvironmentRule>,
    pub ignored_lines: usize,
}

impl From<&SessionEnvironment> for Observation {
    fn from(environment: &SessionEnvironment) -> Self {
        Observation::object([
            ("rules", Observation::from(&environment.rules)),
            ("variables", Observation::from(&environment.variables)),
        ])
    }
}

impl From<&VariablesFile> for Observation {
    fn from(file: &VariablesFile) -> Self {
        Observation::object([
            ("error", reason_of(&file.status)),
            ("ignored_lines", counted(&file.status, file.ignored_lines)),
            ("path", Observation::text(file.path.as_str())),
            ("status", Observation::text(file.status.as_str())),
            (
                "variables",
                match file.status {
                    FileStatus::Ok => {
                        Observation::object(file.variables.iter().map(|(name, value)| {
                            (name.as_str(), Observation::text(value).sensitive())
                        }))
                    }
                    _ => Observation::null(),
                },
            ),
        ])
    }
}

impl From<&RulesFile> for Observation {
    fn from(file: &RulesFile) -> Self {
        Observation::object([
            ("error", reason_of(&file.status)),
            ("ignored_lines", counted(&file.status, file.ignored_lines)),
            ("path", Observation::text(file.path.as_str())),
            (
                "rules",
                match file.status {
                    FileStatus::Ok => Observation::list(file.rules.iter().map(Observation::from)),
                    _ => Observation::null(),
                },
            ),
            ("status", Observation::text(file.status.as_str())),
        ])
    }
}

/// `null` rather than `0` where nothing was read, so a file that is not there cannot be
/// mistaken for one that parsed cleanly.
fn counted(status: &FileStatus, lines: usize) -> Observation {
    match status {
        FileStatus::Ok => Observation::integer(i64::try_from(lines).unwrap_or(i64::MAX)),
        _ => Observation::null(),
    }
}

fn reason_of(status: &FileStatus) -> Observation {
    match status.reason() {
        Some(why) => Observation::text(why),
        None => Observation::null(),
    }
}
