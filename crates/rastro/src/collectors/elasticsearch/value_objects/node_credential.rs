//! The credential a run sends Elasticsearch, and the account whose nodes may be sent it.

use crate::collectors::elasticsearch::value_objects::ApiCredential;
use crate::credentials::Credentials;

const NODE_UID: &str = "ELASTICSEARCH_NODE_UID";

/// A credential, and the one account whose nodes it is sent to.
///
/// **Bound to an account**, found by the security review: a process is taken for a node by its
/// argv, which any account can write, so any account could start one, real binaries and all, and
/// be sent the operator's credential. The account a process runs as is the kernel's to say. See
/// `docs/decisions.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeCredential {
    credential: ApiCredential,
    recipient: u32,
}

impl NodeCredential {
    pub fn new(credential: ApiCredential, recipient: u32) -> Self {
        Self {
            credential,
            recipient,
        }
    }

    /// The Elasticsearch credential among the operator's and its account, if one was given.
    ///
    /// Either without the other is a refusal: a credential with nowhere it may go, or an account
    /// with nothing to give it.
    pub fn from_credentials(credentials: &Credentials) -> Result<Option<Self>, String> {
        let credential = ApiCredential::from_credentials(credentials)?;
        let recipient = credentials.get(NODE_UID);

        match (credential, recipient) {
            (None, None) => Ok(None),
            (Some(credential), Some(recipient)) => recipient
                .parse()
                .map(|recipient| Some(Self::new(credential, recipient)))
                .map_err(|_| format!("{NODE_UID} is not a numeric user id")),
            (Some(_), None) => Err(format!(
                "the credentials give an Elasticsearch credential without {NODE_UID}, the user \
                 id of the account its nodes run as, `id -u elasticsearch` on a package install"
            )),
            (None, Some(_)) => Err(format!(
                "the credentials give {NODE_UID} without an Elasticsearch credential to send"
            )),
        }
    }

    pub fn credential(&self) -> &ApiCredential {
        &self.credential
    }

    /// The user id a node's process must run as to be sent the credential.
    pub fn recipient(&self) -> u32 {
        self.recipient
    }
}
