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

impl From<&Setting> for Observation {
    fn from(setting: &Setting) -> Self {
        // Redaction is a property of the name, not the value: a value that merely looks like
        // a secret is left alone, and a named credential-bearing setting is withheld even
        // when the box happens to have left it empty.
        let value = Observation::text(setting.value.as_str());
        let value = if setting.name.holds_credential() {
            value.sensitive()
        } else {
            value
        };

        Observation::object([
            ("value", value),
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
