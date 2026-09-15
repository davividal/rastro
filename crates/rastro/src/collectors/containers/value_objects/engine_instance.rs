//! Which of a flavour's engines this is.

use rastro_collector::{CollectionError, NonEmptyText};

/// The account an engine belongs to, which is what tells two engines of one flavour apart.
///
/// **One flavour is not one engine, and podman is why.** Every user on a box can run their
/// own `podman system service` with its own store, so `alice`'s `web` and `bob`'s `web` are
/// different containers, and root may be running none at all. A key that held "the podman on
/// this box" would have to pick one of them and call it the engine.
///
/// The owning account is the instance's identity because it is what actually separates them:
/// the store, the socket and the containers all belong to that user. It reads well for the
/// ordinary case too, where a box has exactly one and it is `root`.
///
/// **It generalises beyond podman**, which is the other reason to spend a key on it. Rootless
/// docker has the same shape, and a box where root runs dockerd while a user runs their own
/// would otherwise have nowhere to put the second.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct EngineInstance(NonEmptyText);

/// The account a system engine belongs to, and the only instance on an ordinary box.
const ROOT: &str = "root";

impl EngineInstance {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        let text = NonEmptyText::new(value, "engine instance")?;

        if text.as_str().chars().any(char::is_whitespace) {
            return Err(CollectionError::new(format!(
                "the host reported the account {:?}, and an account name holding whitespace \
                 means the answer was misread",
                text.as_str()
            )));
        }

        Ok(Self(text))
    }

    /// The system engine's instance: dockerd, containerd and a rootful podman all belong to
    /// root.
    pub fn root() -> Self {
        Self::new(ROOT).expect("`root` is a legal account name")
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}
