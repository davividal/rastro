//! What a localisation setting is set to.

use rastro_collector::Observation;

/// A localisation variable's value, as the file spells it.
///
/// `C.UTF-8`, `en_GB.UTF-8`, `de-latin1-nodeadkeys`.
///
/// **Quotes are stripped and emptiness is allowed**, which are the two things these files
/// do that a naive reader gets wrong. `LANG="en_GB.UTF-8"` and `LANG=en_GB.UTF-8` set the
/// same locale, because the files are read as shell fragments, so keeping the quotes would
/// make one box differ from another over punctuation. And `LANG=` is legal and means the
/// variable is explicitly unset, which is not the same as the variable being absent — so
/// this is not built on a non-empty text type.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct SettingValue(String);

impl SettingValue {
    /// Reads the value, without the quotes the file may wrap it in.
    ///
    /// Only a *matched* pair is stripped, and only one. A value with a quote on one side is
    /// left alone: that is a broken file rather than a quoted value, and half-repairing it
    /// would hide the breakage.
    pub fn new(value: &str) -> Self {
        let value = value.trim();

        for quote in ['"', '\''] {
            if let Some(inner) = value
                .strip_prefix(quote)
                .and_then(|rest| rest.strip_suffix(quote))
            {
                return Self(inner.to_owned());
            }
        }

        Self(value.to_owned())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&SettingValue> for Observation {
    fn from(value: &SettingValue) -> Self {
        Observation::text(value.as_str())
    }
}
