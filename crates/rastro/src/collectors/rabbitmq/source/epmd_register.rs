//! The `epmd -names` interface.
//!
//! The Erlang port mapper daemon's register of the nodes on this box: one row per node,
//! carrying the name a CLI tool has to be given and the port that node accepts distribution
//! connections on.
//!
//! **Why this rather than `/proc`.** A pure read of the process table looked like the safer
//! register and cannot serve as one: the broker's beam holds *zero* environment variables and
//! its argv carries no node name at all, so `/proc` can say a node is running and never say
//! what it is called. epmd is the only non-mutating interface that names it.
//!
//! **Why this is allowed to run before anything is known to be up.** Measured on a box with
//! no epmd: `epmd -names` exits 1 in 2 ms with `Cannot connect to local epmd` and starts
//! nothing. Every RabbitMQ CLI tool measured in the same conditions started a daemon that
//! outlived the call. The whole difference between a read and a mutation, in this facet,
//! is which of the two rastro reaches for first.

use rastro_collector::CollectionError;

/// What epmd prints before its rows, and the only evidence that it answered at all.
///
/// Required rather than skipped, because the alternative conflates two opposite states:
/// `epmd: Cannot connect to local epmd` has no rows either, and reading it as an empty
/// register would report a box with no port mapper as a box with no RabbitMQ.
const BANNER: &str = "epmd: up and running";

/// The tokens a row carries: `name <name> at port <port>`.
const ROW_TOKENS: usize = 5;

/// The token a row begins with, which is what makes it a row rather than a message.
const ROW_LEAD: &str = "name";

/// One Erlang node as epmd registered it, before anything has been asked of it.
///
/// Deliberately not a node of the facet: that carries what a node reported about itself,
/// and this is what the box knows without asking. Keeping them apart is what lets the whole
/// dispatch be tested from a fixture with no broker installed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisteredNode {
    /// The local part of the node's name, as epmd spells it.
    ///
    /// Not the facet's key: a CLI tool is addressed at `name@host`, so the key needs the
    /// host as well, which epmd does not print and the box knows for itself.
    pub name: String,

    /// The port this node accepts distribution connections on.
    ///
    /// Recorded because it is the one thread back to a process: the holder of this port is
    /// the node, which is how a register entry is tied to a beam that is really running.
    pub distribution_port: u16,
}

/// What epmd knows about this box.
pub struct EpmdRegister;

impl EpmdRegister {
    /// Reads `epmd -names` output into the nodes it registered.
    ///
    /// **Sorted by name, because epmd's own order is registration order.** It prints nodes in
    /// the order they happened to come up, which differs between two boots of a box nobody
    /// changed. A list order that moves on its own is exactly what the document's determinism
    /// contract forbids, and sorting here means no caller can inherit it.
    ///
    /// An epmd with nothing registered is an ordinary state rather than a failure: the daemon
    /// outlives the node that started it, so a stopped broker leaves precisely this.
    pub fn parse(output: &str) -> Result<Vec<RegisteredNode>, CollectionError> {
        if !output.lines().any(|line| line.starts_with(BANNER)) {
            return Err(CollectionError::new(format!(
                "epmd did not answer with {BANNER:?}, so this is not a register of the \
                 nodes on this box: {output:?}"
            )));
        }

        let mut nodes = Vec::new();
        for line in output.lines() {
            let tokens: Vec<&str> = line.split_whitespace().collect();

            if tokens.first() != Some(&ROW_LEAD) {
                continue;
            }

            nodes.push(node_of(&tokens, line)?);
        }

        nodes.sort_by(|left, right| left.name.cmp(&right.name));

        Ok(nodes)
    }
}

/// One row's node, or the reason the row was not one.
///
/// The token count is the whole validation, and it carries the empty name with it: epmd
/// prints `name <name> at port <port>`, whitespace splitting collapses an empty name rather
/// than leaving a gap, so a row naming nothing arrives one token short and is refused here
/// rather than recorded as a node nobody can address.
fn node_of(tokens: &[&str], line: &str) -> Result<RegisteredNode, CollectionError> {
    if tokens.len() != ROW_TOKENS {
        return Err(CollectionError::new(format!(
            "epmd printed a row of {} tokens where a node takes {ROW_TOKENS}: {line:?}",
            tokens.len()
        )));
    }

    let port = tokens[4].parse().map_err(|_| {
        CollectionError::new(format!(
            "epmd printed the distribution port {:?}, which is not a port: {line:?}",
            tokens[4]
        ))
    })?;

    Ok(RegisteredNode {
        name: tokens[1].to_owned(),
        distribution_port: port,
    })
}
