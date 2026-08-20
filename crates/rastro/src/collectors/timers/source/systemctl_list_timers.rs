//! The `systemctl list-timers` interface.

use rastro_collector::CollectionError;

use super::systemctl_timers::TimerRow;
use crate::collectors::canonical_tool::CanonicalTool;
use crate::collectors::timers::model::TimerTable;

const PROGRAM: &str = "systemctl";

/// Ask for JSON, so the shape is rastro's rather than systemd's to format.
const JSON: &str = "--output=json";

const NO_PAGER: &str = "--no-pager";

/// Without this, only timers with a future elapse are listed.
///
/// A timer that has stopped firing is exactly what a diff should surface, so the ones
/// with no next elapse are the interesting half rather than clutter.
const ALL: &str = "--all";

/// systemd's timer list as a source rastro can read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemctlTimers {
    tool: CanonicalTool,
}

impl SystemctlTimers {
    /// Finds systemd's control tool, or reports that this host does not run systemd.
    pub fn detect() -> Option<Self> {
        CanonicalTool::located(PROGRAM).map(Self::using)
    }

    /// The same over a tool the caller located, so the argument vector is reachable from
    /// a test rather than being the one part of the exec route nothing can observe.
    pub fn using(tool: CanonicalTool) -> Self {
        Self { tool }
    }

    pub fn tool(&self) -> &CanonicalTool {
        &self.tool
    }

    pub fn read(&self) -> Result<TimerTable, CollectionError> {
        Self::parse(&self.tool.run(&["list-timers", ALL, JSON, NO_PAGER])?)
    }

    /// Translates the tool's output into the model.
    ///
    /// Separate from [`Self::read`] so the whole translation is exercised from a fixture,
    /// with no systemd to run.
    pub fn parse(output: &str) -> Result<TimerTable, CollectionError> {
        let rows: Vec<TimerRow> = serde_json::from_str(output).map_err(|error| {
            CollectionError::new(format!(
                "could not read what `{PROGRAM} list-timers` reported as JSON: {error}"
            ))
        })?;

        TimerTable::new(
            rows.iter()
                .map(TimerRow::to_timer)
                .collect::<Result<Vec<_>, CollectionError>>()?,
        )
    }
}
