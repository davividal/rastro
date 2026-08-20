//! What the box is localised to.

use std::collections::BTreeMap;

use rastro_collector::{AbsolutePath, CollectionError, Observation};

use crate::collectors::locale::value_objects::{SettingName, SettingValue};

/// The localisation configuration, by the file each setting came from.
///
/// # Why the files rather than `localectl`
///
/// `localectl` is the tool that owns this state, and it was rejected on what it prints.
/// systemd 252 has no `localectl show`: the verb does not exist, which was checked on the
/// box rather than assumed. What it does have is `localectl status`, an aligned human table
/// whose values are labels like `System Locale:` and whose unset fields read `(unset)`, and
/// whose multi-variable form folds onto continuation lines that the development box — with
/// only `LANG` set — cannot exercise. Writing a parser for a shape rastro could not verify
/// is exactly the wrong trade.
///
/// The files are unambiguous, and unlike `sysctl` there is no runtime resolution being
/// missed: `localectl` reads these same files and writes them back. This is the same
/// judgement the repositories facet makes about apt, and it is recorded in both places.
///
/// # Why keyed by file
///
/// **Because which file a setting is in changes what it does, and the two on a Debian box do
/// not agree.** `/etc/locale.conf` is systemd's; `/etc/default/locale` is Debian's, and it is
/// the one that exists here. Merging them into one map would need rastro to invent a
/// precedence, and reporting each file's own contents states what is there without deciding
/// anything.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Localisation(BTreeMap<String, Option<BTreeMap<SettingName, SettingValue>>>);

impl Localisation {
    /// Files each source's settings under its path.
    ///
    /// A file that is not on the host is present as a key with no settings rather than
    /// missing, for the reason the packages inventory gives: a document silent about
    /// `/etc/locale.conf` cannot be told from one written before rastro read it, while one
    /// saying the file is not there states a fact about the host.
    pub fn new(
        files: impl IntoIterator<Item = (AbsolutePath, Option<BTreeMap<SettingName, SettingValue>>)>,
    ) -> Result<Self, CollectionError> {
        let mut filed = BTreeMap::new();

        for (path, settings) in files {
            if filed.insert(path.as_str().to_owned(), settings).is_some() {
                return Err(CollectionError::new(format!(
                    "{} was read twice",
                    path.as_str()
                )));
            }
        }

        Ok(Self(filed))
    }

    pub fn files(&self) -> &BTreeMap<String, Option<BTreeMap<SettingName, SettingValue>>> {
        &self.0
    }
}

impl From<&Localisation> for Observation {
    fn from(localisation: &Localisation) -> Self {
        Observation::object(localisation.files().iter().map(|(path, settings)| {
            let reported = match settings {
                Some(settings) => Observation::object(
                    settings
                        .iter()
                        .map(|(name, value)| (name.as_str(), Observation::from(value))),
                ),
                None => Observation::null(),
            };

            (path.as_str(), reported)
        }))
    }
}
