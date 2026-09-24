//! Which node on the box.

use std::cmp::Ordering;

use rastro_collector::CollectionError;

/// The separator Erlang puts between a node's local name and its host.
const SEPARATOR: char = '@';

/// A node's name, exactly as the node runs under it.
///
/// **Read from the box, never composed.** An earlier version built this from the register's
/// local part and the box's hostname, which is a guess wearing a reading's clothes: a node
/// started with long names calls itself `rabbit@broker.example.test` while that composition
/// says `rabbit@broker`, so the facet would have keyed on a name nothing answers to and
/// addressed `rabbitmqctl -n` with it. The broker writes its own name into the directories it
/// keeps open, and that is where this comes from.
///
/// Held as the whole name, because that is what it is: what the node calls itself, what
/// `-n` takes, and what this facet keys on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeName(String);

impl NodeName {
    /// A node's name as the box spells it.
    ///
    /// Both halves must be there, because `@box` and `rabbit@` address nothing, and exactly
    /// one separator, because `a@b@c` is not a name any Erlang node answers to.
    pub fn parse(name: impl Into<String>) -> Result<Self, CollectionError> {
        let name = name.into();
        let halves: Vec<&str> = name.split(SEPARATOR).collect();

        let named = halves.len() == 2 && halves.iter().all(|half| !half.is_empty());
        if !named {
            return Err(CollectionError::new(format!(
                "a node is named local{SEPARATOR}host, and {name:?} is not"
            )));
        }

        Ok(Self(name))
    }

    /// The whole name, which is also the facet key.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether this node runs under long names, which decides how it must be addressed.
    ///
    /// **A dot in the host half is the whole test**, because that is Erlang's own rule:
    /// `-sname` refuses a host containing one, so a name that has one came from `-name`.
    ///
    /// **Measured, and the flag is not safe to pass defensively.** Against a long-name node
    /// `rabbitmqctl -n rabbit@broker.example.test status` exits 65 with `invalid node name`,
    /// and with `--longnames` it answers. Against a *short*-name node the same flag makes the
    /// tool hang until it is killed: exit 124 on a 20-second bound, where the same call
    /// without it answers immediately. So this is a question that has to be asked per node
    /// rather than a flag that can be set once and forgotten.
    pub fn uses_long_names(&self) -> bool {
        self.0
            .split_once(SEPARATOR)
            .is_some_and(|(_, host)| host.contains('.'))
    }
}

/// Ordered by the name, so this type's order and the document's are one order.
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
