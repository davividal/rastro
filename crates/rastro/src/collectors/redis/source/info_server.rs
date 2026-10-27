//! The `INFO server` reply.

use std::collections::BTreeMap;

use rastro_collector::CollectionError;

use super::info_fields::info_fields;
use crate::collectors::redis::model::ServerIdentity;
use crate::collectors::redis::value_objects::ServerKind;

/// Where valkey names itself, and the version it means.
///
/// Valkey still prints `redis_version`, frozen at 7.2.4, the release it forked from, so that
/// field describes a valkey server wrongly. Measured on the official image.
const SERVER_NAME: &str = "server_name";
const VALKEY_NAME: &str = "valkey";
const VALKEY_VERSION: &str = "valkey_version";
const REDIS_VERSION: &str = "redis_version";

/// The two spellings of the mode: redis's, and valkey's rename of it.
const MODE_FIELDS: [&str; 2] = ["redis_mode", "server_mode"];

const EXECUTABLE: &str = "executable";
const CONFIG_FILE: &str = "config_file";

/// The reader of `INFO server`.
pub struct InfoServer;

impl InfoServer {
    /// What the server says it is.
    ///
    /// A reply with no version is refused: whatever answered is not describing itself as a redis
    /// or a valkey, and a document naming it as one would be a guess.
    pub fn parse(text: &str) -> Result<ServerIdentity, CollectionError> {
        let fields = info_fields(text);

        let kind = match fields.get(SERVER_NAME).map(String::as_str) {
            Some(VALKEY_NAME) => ServerKind::Valkey,
            _ => ServerKind::Redis,
        };
        let version_field = match kind {
            ServerKind::Valkey => VALKEY_VERSION,
            ServerKind::Redis => REDIS_VERSION,
        };
        let version = fields.get(version_field).cloned().ok_or_else(|| {
            CollectionError::new(format!(
                "the server's INFO carries no {version_field}, so it is not describing itself as \
                 a {}",
                kind.as_str()
            ))
        })?;

        Ok(ServerIdentity {
            kind,
            version,
            mode: MODE_FIELDS
                .iter()
                .find_map(|field| non_empty(&fields, field)),
            executable: non_empty(&fields, EXECUTABLE),
            config_file: non_empty(&fields, CONFIG_FILE),
        })
    }
}

/// A field's value, where the server printed one; redis prints `config_file:` with nothing after
/// it for a server started without a file.
fn non_empty(fields: &BTreeMap<String, String>, name: &str) -> Option<String> {
    fields.get(name).filter(|value| !value.is_empty()).cloned()
}
