//! Which node on the box.

use std::cmp::Ordering;

use rastro_collector::{CollectionError, NonEmptyText};

/// The separator Erlang puts between a node's local name and its host.
const SEPARATOR: char = '@';

/// A node's full name, which is the name a CLI tool has to be addressed with.
///
/// **Composed from two sources, because neither knows both halves.** epmd prints the local
/// part and nothing else; the host comes from the box. `rabbit@host` is what the node calls
/// itself, what `rabbitmqctl -n` takes, and what this facet keys on, so it is held as the
/// rendered form rather than as its parts.
///
/// **A host of the short form, which is what a default installation uses.** A node started
/// under long names is `rabbit@host.example.com` and would be addressed with `--longnames`
/// as well; rastro does not compose that yet, and the node's own account of its name is
/// recorded beside this one so a disagreement is visible rather than silent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeName(String);

impl NodeName {
    pub fn new(local: impl Into<String>, host: impl Into<String>) -> Result<Self, CollectionError> {
        let local = NonEmptyText::new(local, "node name")?;
        let host = NonEmptyText::new(host, "node host")?;

        // Refused rather than rendered, because a second separator makes the composed name
        // ambiguous: `a@b@c` cannot be split back into the halves that made it, and the
        // halves are what a CLI tool is addressed with.
        for half in [local.as_str(), host.as_str()] {
            if half.contains(SEPARATOR) {
                return Err(CollectionError::new(format!(
                    "a node name is composed around {SEPARATOR:?}, so neither half may \
                     contain one: {half:?}"
                )));
            }
        }

        Ok(Self(format!(
            "{}{SEPARATOR}{}",
            local.as_str(),
            host.as_str()
        )))
    }

    /// The `local@host` form, which is also the facet key.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Ordered by the rendered name, so this type's order and the document's are one order.
///
/// The same reasoning [`ClusterId`](crate::collectors::postgresql::value_objects::ClusterId)
/// records: the facet renders as an object whose key order the format decides
/// lexicographically, so any cleverer ordering here would govern an internal map and nothing
/// a reader sees.
impl Ord for NodeName {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.cmp(&other.0)
    }
}

impl PartialOrd for NodeName {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
