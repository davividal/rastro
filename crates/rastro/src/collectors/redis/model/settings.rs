//! What a server is running with.

use std::collections::BTreeMap;

use rastro_collector::Observation;

use crate::collectors::redis::value_objects::SettingName;

/// Every setting a server reports, by name.
///
/// **The running values, not the file's.** On the estate this facet was written for, `maxmemory`
/// and `save` are applied with `CONFIG SET` and written nowhere, so the file and the server
/// disagree by design; these are the values the server is using.
///
/// **Text, as the server spells it.** Every value arrives as a string, `maxmemory` in bytes and
/// `save` as its rule list, and re-typing one spelling would invent a shape the server did not
/// report. An empty value is a value: `save ""` is persistence switched off.
///
/// **A credential is carried and marked sensitive**, so the default document shows a digest that
/// changes when the password does and `--raw` shows the password. A digest of an unsalted
/// password proves a rotation and does not hide a guessable one, which `SECURITY.md` says of
/// redaction everywhere.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Settings {
    pub values: BTreeMap<SettingName, String>,
}

impl From<&Settings> for Observation {
    fn from(settings: &Settings) -> Self {
        Observation::object(settings.values.iter().map(|(name, value)| {
            let observed = Observation::text(value.as_str());

            (
                name.as_str(),
                // An empty credential is an unset one, and a digest of it would say a secret
                // exists; measured, `masterauth` is `""` on every server that is no replica.
                match name.holds_credential() && !value.is_empty() {
                    true => observed.sensitive(),
                    false => observed,
                },
            )
        }))
    }
}
