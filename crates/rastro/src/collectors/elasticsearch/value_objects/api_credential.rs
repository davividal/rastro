//! The credential a run sends Elasticsearch.

use std::fmt;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use crate::credentials::Credentials;

pub(super) const API_KEY: &str = "ELASTICSEARCH_API_KEY";
pub(super) const USERNAME: &str = "ELASTICSEARCH_USERNAME";
pub(super) const PASSWORD: &str = "ELASTICSEARCH_PASSWORD";

/// What a run authenticates to every Elasticsearch node on the box with.
///
/// **One for the box, in v1**: telling two clusters' credentials apart needs the cluster's name,
/// and the name is behind the credential. A node of another cluster rejects it, and is reported
/// as not read. See `docs/decisions.md`.
#[derive(Clone, PartialEq, Eq)]
pub enum ApiCredential {
    /// An API key in the encoded form Elasticsearch hands out, `base64(id:key)`.
    ApiKey(String),
    Basic {
        username: String,
        password: String,
    },
}

impl ApiCredential {
    pub fn api_key(encoded: impl Into<String>) -> Self {
        Self::ApiKey(encoded.into())
    }

    /// The Elasticsearch credential among the operator's, if one was given.
    ///
    /// An API key, or a username with its password. Half a credential, or both kinds, is a
    /// refusal rather than a pick: which one the operator meant cannot be told.
    pub fn from_credentials(credentials: &Credentials) -> Result<Option<Self>, String> {
        let api_key = credentials.get(API_KEY);
        let username = credentials.get(USERNAME);
        let password = credentials.get(PASSWORD);

        match (api_key, username, password) {
            (None, None, None) => Ok(None),
            (Some(key), None, None) => Ok(Some(Self::api_key(key))),
            (None, Some(username), Some(password)) => Ok(Some(Self::Basic {
                username: username.to_owned(),
                password: password.to_owned(),
            })),
            (Some(_), _, _) => Err(format!(
                "the credentials give {API_KEY} and a username or password beside it; give one"
            )),
            (None, Some(_), None) => Err(format!(
                "the credentials give {USERNAME} without {PASSWORD}"
            )),
            (None, None, Some(_)) => Err(format!(
                "the credentials give {PASSWORD} without {USERNAME}"
            )),
        }
    }

    /// The `Authorization` header's value.
    pub fn authorization(&self) -> String {
        match self {
            Self::ApiKey(encoded) => format!("ApiKey {encoded}"),
            Self::Basic { username, password } => {
                format!(
                    "Basic {}",
                    STANDARD.encode(format!("{username}:{password}"))
                )
            }
        }
    }
}

/// Which kind only: a `Debug` that showed the secret would put it in any panic that prints one.
impl fmt::Debug for ApiCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ApiKey(_) => formatter.write_str("ApiCredential::ApiKey(..)"),
            Self::Basic { .. } => formatter.write_str("ApiCredential::Basic(..)"),
        }
    }
}
