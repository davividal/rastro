//! The `CONFIG GET *` reply: a flat array of names and values, in no particular order.

use std::collections::BTreeMap;

use rastro_collector::CollectionError;

use super::reply::Reply;
use crate::collectors::redis::model::Settings;
use crate::collectors::redis::value_objects::SettingName;

/// The reader of `CONFIG GET *`.
pub struct ConfigGet;

impl ConfigGet {
    /// The settings, keyed by name.
    ///
    /// **Refused whole rather than read in part.** A reply that is not name-value text pairs was
    /// misread or is not from a redis, and keeping the pairs that happened to parse would put a
    /// server in the document with settings missing and nothing saying so.
    pub fn parse(reply: Reply) -> Result<Settings, CollectionError> {
        let Reply::Array(elements) = reply else {
            return Err(CollectionError::new(format!(
                "the server answered CONFIG GET * with {reply:?} rather than a list of settings"
            )));
        };

        if elements.len() % 2 != 0 {
            return Err(CollectionError::new(
                "the server's CONFIG GET * has a name without a value, so it was misread",
            ));
        }

        let mut values = BTreeMap::new();
        let mut elements = elements.into_iter();
        while let (Some(name), Some(value)) = (elements.next(), elements.next()) {
            let (Reply::Bulk(name), Reply::Bulk(value)) = (name, value) else {
                return Err(CollectionError::new(
                    "the server's CONFIG GET * holds something other than text, so it was \
                     misread",
                ));
            };

            values.insert(SettingName::new(name)?, value);
        }

        Ok(Settings { values })
    }
}
