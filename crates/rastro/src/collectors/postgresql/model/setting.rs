//! One server setting, in rastro's terms.

use rastro_collector::Observation;

use crate::collectors::postgresql::value_objects::{
    SettingName, SettingSource, SettingUnit, SettingValue,
};

/// A setting as the running server reports it.
///
/// A plain aggregate: every field is already a validated value. It carries `source` and
/// `pending_restart` because both describe the host rather than the value, and both are
/// what a diff of two clusters is actually read for: whether somebody chose this, and
/// whether the running server has taken it up yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Setting {
    pub name: SettingName,
    pub value: SettingValue,
    pub unit: Option<SettingUnit>,
    pub source: SettingSource,

    /// The server has read a new value from its configuration and needs a restart to use it.
    ///
    /// **Only after a reload, and that limit is worth stating.** The server sets this when it
    /// re-reads its configuration and finds a value it cannot adopt without restarting. A
    /// file edited and never reloaded leaves it `false`, because the server does not yet know
    /// the file changed: on the reference box a `conf.d` drop-in asked for three
    /// `shared_preload_libraries` and `pg_settings` reported the default with
    /// `pending_restart` false, the cluster having run unreloaded since it was written.
    ///
    /// So this is drift no file comparison can see, and the walker is what sees the drift
    /// this cannot.
    pub pending_restart: bool,

    /// When a change to this setting takes effect: `postmaster`, `sighup`, `user`, and the
    /// rest of the GUC contexts.
    ///
    /// Recorded because it tells a reader of a diff whether a changed value could have taken
    /// effect at all: a `postmaster`-context setting that moved means the cluster was
    /// restarted, a `user`-context one means nothing of the kind.
    pub context: String,

    /// The file a value was set in, or `None` where it came from a default, the command line,
    /// or a role too unprivileged to be shown the source.
    pub sourcefile: Option<String>,

    /// The line of [`Setting::sourcefile`] the value was set on, absent on the same terms.
    pub sourceline: Option<i64>,
}

/// Renders a setting's value, withholding the content of one that can carry a credential.
///
/// While rastro's redaction layer is unbuilt, marking a value `sensitive` does not stop the
/// renderer emitting it, so the collector withholds the content itself rather than shipping a
/// secret in cleartext: a credential-bearing setting reports only whether it is set. The
/// accounts collector makes the same choice for a password hash. The cost, accepted
/// knowingly, is that a change *within* the value is invisible; a value appearing or being
/// removed still shows, and the `sensitive` marker stays for the day a redaction layer can do
/// better.
pub(crate) fn value_observation(name: &SettingName, value: &SettingValue) -> Observation {
    if name.holds_credential() {
        let presence = if value.as_str().is_empty() {
            ""
        } else {
            "[redacted]"
        };

        return Observation::text(presence).sensitive();
    }

    Observation::text(value.as_str())
}

impl From<&Setting> for Observation {
    fn from(setting: &Setting) -> Self {
        Observation::object([
            ("value", value_observation(&setting.name, &setting.value)),
            (
                "unit",
                match &setting.unit {
                    Some(unit) => Observation::text(unit.as_str()),
                    None => Observation::null(),
                },
            ),
            ("source", Observation::text(setting.source.as_str())),
            (
                "pending_restart",
                Observation::boolean(setting.pending_restart),
            ),
            ("context", Observation::text(setting.context.as_str())),
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
        ])
    }
}
