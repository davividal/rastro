//! The `MODULE LIST` reply: one array per module, of alternating field names and values.
//!
//! ```text
//! 1) 1) "name"  2) "ReJSON"  3) "ver"  4) (integer) 20609  5) "path"  6) "/usr/lib/…"  7) "args"  8) (empty)
//! ```

use std::collections::BTreeMap;

use rastro_collector::CollectionError;

use super::reply::Reply;
use crate::collectors::redis::model::{Module, Modules};

/// The reader of `MODULE LIST`.
pub struct ModuleList;

impl ModuleList {
    /// The loaded modules, by name.
    ///
    /// A field this does not know is passed over rather than refused: valkey and later redis
    /// releases add to the description, and a module's name and version are what it is.
    pub fn parse(reply: Reply) -> Result<Modules, CollectionError> {
        let Reply::Array(entries) = reply else {
            return Err(misread(format!(
                "answered with {reply:?} rather than a list"
            )));
        };

        let mut loaded = BTreeMap::new();
        for entry in entries {
            let (name, module) = module_of(entry)?;
            if loaded.insert(name.clone(), module).is_some() {
                return Err(misread(format!("names {name} twice")));
            }
        }

        Ok(Modules { loaded })
    }
}

fn module_of(entry: Reply) -> Result<(String, Module), CollectionError> {
    let Reply::Array(fields) = entry else {
        return Err(misread(
            "has a module that is not a list of fields".to_owned(),
        ));
    };

    let mut name = None;
    let mut version = None;
    let mut path = None;
    let mut args = None;

    let mut fields = fields.into_iter();
    while let (Some(field), Some(value)) = (fields.next(), fields.next()) {
        let Reply::Bulk(field) = field else {
            return Err(misread("has a field name that is not text".to_owned()));
        };

        match (field.as_str(), value) {
            ("name", Reply::Bulk(value)) => name = Some(value),
            ("ver", Reply::Integer(value)) => version = Some(value),
            // Empty for a module built into the server, measured on redis 8's `vectorset`.
            ("path", Reply::Bulk(value)) => path = Some(value).filter(|path| !path.is_empty()),
            ("args", Reply::Array(values)) => args = Some(texts_of(values)?),
            ("name" | "ver" | "path" | "args", _) => {
                return Err(misread(format!("has a {field} of an unexpected kind")));
            }
            _ => {}
        }
    }

    let name = name.ok_or_else(|| misread("has a module with no name".to_owned()))?;
    let version = version.ok_or_else(|| misread(format!("gives {name} no version")))?;

    Ok((
        name,
        Module {
            version,
            path,
            args,
        },
    ))
}

fn texts_of(values: Vec<Reply>) -> Result<Vec<String>, CollectionError> {
    values
        .into_iter()
        .map(|value| match value {
            Reply::Bulk(text) => Ok(text),
            _ => Err(misread("has an argument that is not text".to_owned())),
        })
        .collect()
}

fn misread(what: String) -> CollectionError {
    CollectionError::new(format!(
        "the server's MODULE LIST {what}, so it was misread"
    ))
}
