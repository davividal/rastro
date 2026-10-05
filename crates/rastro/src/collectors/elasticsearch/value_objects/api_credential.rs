//! The credential a run sends Elasticsearch.

use std::fmt;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use crate::credentials::Credentials;

const API_KEY: &str = "ELASTICSEARCH_API_KEY";
const USERNAME: &str = "ELASTICSEARCH_USERNAME";
const PASSWORD: &str = "ELASTICSEARCH_PASSWORD";

/// Every name the Elasticsearch credential is given by; any other under the prefix is a typo.
const PREFIX: &str = "ELASTICSEARCH_";
const KNOWN: [&str; 3] = [API_KEY, USERNAME, PASSWORD];

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
        // Found by review: a misspelt name read as no credential, and every secured node as
        // not read, for a typo the run never mentioned.
        if let Some(unknown) = credentials
            .names()
            .into_iter()
            .find(|name| name.starts_with(PREFIX) && !KNOWN.contains(&name.as_str()))
        {
            return Err(format!(
                "the credentials give {unknown}, which is not a name rastro reads; it reads {}",
                KNOWN.join(", ")
            ));
        }
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
