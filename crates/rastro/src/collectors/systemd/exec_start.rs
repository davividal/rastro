//! One command a unit starts.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

use super::executable_path::ExecutablePath;

/// A resolved `ExecStart=`: the binary, and the argument vector it is given.
///
/// # Why the argument vector is one string and not a list
///
/// **systemd does not preserve the quoting in what it shows.** A unit reading
/// `ExecStart=/bin/echo --flag="a b" second` comes back as `argv[]=/bin/echo --flag=a b
/// second`, so three whitespace-separated tokens stand for two arguments and nothing in the
/// output says which. That was measured against systemd 252 rather than assumed.
///
/// Splitting it here would therefore claim a structure the source cannot support, and be
/// silently wrong for exactly the units whose arguments are interesting. Keeping the line
/// whole is the honest record; a collector that knows one program's flags well enough for a
/// bad split to be *refutable* can tokenise it itself, which is what the exporters facet
/// does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecStart {
    pub executable: ExecutablePath,
    /// The whole vector including `argv[0]`, which a unit may set to something other than
    /// the executable.
    pub argv: NonEmptyText,
}

impl ExecStart {
    pub fn new(
        executable: impl Into<String>,
        argv: impl Into<String>,
    ) -> Result<Self, CollectionError> {
        Ok(Self {
            executable: ExecutablePath::new(executable)?,
            argv: NonEmptyText::new(argv, "unit argument vector")?,
        })
    }
}

impl From<&ExecStart> for Observation {
    fn from(start: &ExecStart) -> Self {
        Observation::object([
            ("argv", Observation::text(start.argv.as_str())),
            ("executable", Observation::text(start.executable.as_str())),
        ])
    }
}
