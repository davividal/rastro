//! A containerd namespace.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// The name of a containerd namespace: `moby`, `k8s.io`, `default`.
///
/// **containerd's tenancy boundary, and the reason its containers are not simply a list.**
/// Two namespaces may hold containers with the same id, and nothing in one is visible from
/// another. Which ones exist also says who is using the engine: `moby` is docker's, `k8s.io`
/// is a kubelet's, `default` is what `nerdctl` uses.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct NamespaceName(NonEmptyText);

impl NamespaceName {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        let text = NonEmptyText::new(value, "containerd namespace")?;

        if text.as_str().chars().any(char::is_whitespace) {
            return Err(CollectionError::new(format!(
                "containerd reported the namespace {:?}, and a name holding whitespace means \
                 the answer was misread",
                text.as_str()
            )));
        }

        Ok(Self(text))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&NamespaceName> for Observation {
    fn from(name: &NamespaceName) -> Self {
        Observation::text(name.as_str())
    }
}
