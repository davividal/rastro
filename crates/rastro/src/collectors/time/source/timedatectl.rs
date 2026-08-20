//! The `timedatectl show` interface.
//!
//! `Timezone=Etc/UTC`, one setting per line, `=` separated. A flat key-value list, which is
//! why this needs no JSON: there is nothing for a column split to get wrong.

use std::collections::BTreeMap;

use rastro_collector::CollectionError;

use crate::collectors::canonical_tool::CanonicalTool;
use crate::collectors::time::model::ClockSettings;
use crate::collectors::time::value_objects::{Timezone, WallClock};

const PROGRAM: &str = "timedatectl";

const TIMEZONE: &str = "Timezone";
const LOCAL_RTC: &str = "LocalRTC";
const CAN_NTP: &str = "CanNTP";
const NTP: &str = "NTP";
const NTP_SYNCHRONIZED: &str = "NTPSynchronized";
const TIME: &str = "TimeUSec";
const RTC_TIME: &str = "RTCTimeUSec";

/// How systemd spells true in this output.
const YES: &str = "yes";

/// The host's timekeeping, as a source rastro can read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Timedatectl {
    tool: CanonicalTool,
}

impl Timedatectl {
    /// Finds the tool, or reports that this host does not have it.
    pub fn detect() -> Option<Self> {
        CanonicalTool::located(PROGRAM).map(Self::using)
    }

    /// The same over a tool the caller located.
    pub fn using(tool: CanonicalTool) -> Self {
        Self { tool }
    }

    pub fn tool(&self) -> &CanonicalTool {
        &self.tool
    }

    /// `show` rather than `status`, because it prints machine-readable pairs where `status`
    /// prints an aligned human table with a localised weekday in it.
    pub fn read(&self) -> Result<ClockSettings, CollectionError> {
        Self::parse(&self.tool.run(&["show"])?)
    }

    /// Translates the tool's output into the model.
    ///
    /// Separate from [`Self::read`] so the whole translation is exercised from a fixture,
    /// with no systemd to run.
    ///
    /// **Every field is required.** systemd prints all seven unconditionally, so a missing
    /// one means the output is not what rastro believes and guessing a default would put a
    /// setting in the document that the host never reported. `LocalRTC=no` defaulted to
    /// `false` would be a claim about how the hardware clock runs.
    pub fn parse(output: &str) -> Result<ClockSettings, CollectionError> {
        let settings: BTreeMap<&str, &str> = output
            .lines()
            .filter_map(|line| line.split_once('='))
            .map(|(name, value)| (name.trim(), value.trim()))
            .collect();

        Ok(ClockSettings {
            timezone: Timezone::new(field(&settings, TIMEZONE)?)?,
            local_real_time_clock: boolean(&settings, LOCAL_RTC)?,
            can_synchronise: boolean(&settings, CAN_NTP)?,
            synchronisation_enabled: boolean(&settings, NTP)?,
            synchronised: boolean(&settings, NTP_SYNCHRONIZED)?,
            system_clock: WallClock::new(field(&settings, TIME)?)?,
            hardware_clock: WallClock::new(field(&settings, RTC_TIME)?)?,
        })
    }
}

fn field<'a>(
    settings: &BTreeMap<&'a str, &'a str>,
    name: &str,
) -> Result<&'a str, CollectionError> {
    settings.get(name).copied().ok_or_else(|| {
        CollectionError::new(format!("`{PROGRAM} show` reported no {name:?} setting"))
    })
}

/// A yes-or-no setting.
///
/// Anything that is not `yes` is false, which is systemd's own reading of these fields, and
/// the alternative is refusing an output that a future systemd spells `true`.
fn boolean(settings: &BTreeMap<&str, &str>, name: &str) -> Result<bool, CollectionError> {
    Ok(field(settings, name)? == YES)
}
