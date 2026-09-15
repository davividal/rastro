//! One environment file a unit names, and what rastro found in it.

use std::collections::BTreeMap;

use rastro_collector::{EnvironmentVariableName, Observation};

use crate::collectors::systemd::EnvironmentFile;

/// An `EnvironmentFile=` declaration paired with the reading of it.
///
/// **Two facts that a diff wants separately.** The declaration is what the unit says and
/// changes when somebody edits the unit; the reading is what was on disk at the time and
/// changes when somebody edits the file. A facet that merged them would report one event
/// for two different causes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvironmentSource {
    pub declared: EnvironmentFile,
    pub reading: EnvironmentReading,
}

/// What came of opening the file.
///
/// **The facet's own `ok | absent | error` vocabulary, one level down.** A missing file is
/// state and not a failure — it is routine for one marked `ignore_errors`, and it is the
/// whole finding for one that is not, since that unit will not start. A file that is there
/// and would not open is a failure, and saying `absent` for it would be the confident lie
/// the three-valued [`Presence`](rastro_collector::Presence) exists upstream to avoid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvironmentReading {
    Read {
        variables: BTreeMap<EnvironmentVariableName, String>,
        ignored_lines: usize,
    },
    Absent,
    Unreadable(String),
}

impl EnvironmentReading {
    fn status(&self) -> &'static str {
        match self {
            Self::Read { .. } => "ok",
            Self::Absent => "absent",
            Self::Unreadable(_) => "error",
        }
    }
}

impl From<&EnvironmentSource> for Observation {
    fn from(source: &EnvironmentSource) -> Self {
        let (variables, ignored_lines, error) =
            match &source.reading {
                EnvironmentReading::Read {
                    variables,
                    ignored_lines,
                } => (
                    // Names in the clear, values withheld. A name says which variable a service
                    // depends on, which is what an operator moving a box needs and is not itself
                    // a secret. A value is where the credential is.
                    Observation::object(variables.iter().map(|(name, value)| {
                        (name.as_str(), Observation::text(value).sensitive())
                    })),
                    Observation::integer(i64::try_from(*ignored_lines).unwrap_or(i64::MAX)),
                    Observation::null(),
                ),
                EnvironmentReading::Absent => (
                    Observation::null(),
                    Observation::null(),
                    Observation::null(),
                ),
                EnvironmentReading::Unreadable(why) => (
                    Observation::null(),
                    Observation::null(),
                    Observation::text(why.as_str()),
                ),
            };

        Observation::object([
            ("error", error),
            (
                "ignore_errors",
                Observation::boolean(source.declared.ignore_errors),
            ),
            ("ignored_lines", ignored_lines),
            ("path", Observation::text(source.declared.path.as_str())),
            ("status", Observation::text(source.reading.status())),
            ("variables", variables),
        ])
    }
}
