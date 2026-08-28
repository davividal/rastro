//! One line `pg_file_settings` found in a configuration file.

use rastro_collector::Observation;

use super::setting::value_observation;
use crate::collectors::postgresql::value_objects::{SettingName, SettingValue};

/// A setting as it stands in the configuration files, before the server applies it.
///
/// `pg_file_settings` re-parses `postgresql.conf` and every include, so it sees what
/// `pg_settings` cannot: a value edited without a reload, where the file has it and the
/// running server does not, and a line that will not apply at all (`applied = false` with an
/// `error`), the typo'd drop-in that reads as fine in the file. It is the missing half of the
/// collector's own thesis, that a file edited without a reload is a value the server is not
/// using.
///
/// Recorded apart from the effective [`Setting`](super::Setting) rather than compared to it
/// here: the file says `128MB` where the server says `16384` in units of `8kB`, so the two
/// are not comparable field to field, only run to run.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct FileSetting {
    /// The order the server read this line in, across every file. Precedence follows it: a
    /// later line for the same setting wins, which is why the entries are ordered by it.
    pub seqno: i64,
    pub sourcefile: Option<String>,
    pub sourceline: Option<i64>,
    pub name: SettingName,
    pub value: SettingValue,

    /// Whether the server accepted the line. `false` carries an `error`, and is the typo a
    /// file comparison reads straight past.
    pub applied: bool,

    /// Why the line was not applied, where it was not.
    pub error: Option<String>,
}

impl From<&FileSetting> for Observation {
    fn from(setting: &FileSetting) -> Self {
        Observation::object([
            ("seqno", Observation::integer(setting.seqno)),
            (
                "sourcefile",
                match &setting.sourcefile {
                    Some(file) => Observation::text(file.as_str()),
                    None => Observation::null(),
                },
            ),
            (
                "sourceline",
                match setting.sourceline {
                    Some(line) => Observation::integer(line),
                    None => Observation::null(),
                },
            ),
            ("name", Observation::text(setting.name.as_str())),
            ("value", value_observation(&setting.name, &setting.value)),
            ("applied", Observation::boolean(setting.applied)),
            (
                "error",
                match &setting.error {
                    Some(error) => Observation::text(error.as_str()),
                    None => Observation::null(),
                },
            ),
        ])
    }
}
