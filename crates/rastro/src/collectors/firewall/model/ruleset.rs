//! Every table one interface reported.

use std::collections::BTreeMap;

use rastro_collector::{CollectionError, Observation};

use super::firewall_chain::FirewallChain;
use crate::collectors::firewall::value_objects::{ChainName, TableName};

/// One interface's whole ruleset, keyed by table and then by chain.
///
/// Keyed at both levels, because both names are unique within their scope: a table appears
/// once in a dump and a chain once in a table. Only the rules *inside* a chain keep an
/// order, and there the order is the meaning.
///
/// **An empty ruleset is a real and common answer.** With the nftables backend,
/// `iptables-save` prints nothing at all when no table has been created, which was measured
/// on the development box: zero bytes, exit status zero, nothing on stderr. That is a box
/// filtering nothing, and it is exactly the state an operator wants to see turn into
/// something after a firewall is configured.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Ruleset(BTreeMap<TableName, BTreeMap<ChainName, FirewallChain>>);

impl Ruleset {
    pub fn new(
        tables: impl IntoIterator<Item = (TableName, BTreeMap<ChainName, FirewallChain>)>,
    ) -> Result<Self, CollectionError> {
        let mut filed = BTreeMap::new();

        for (name, chains) in tables {
            if filed.insert(name.clone(), chains).is_some() {
                return Err(CollectionError::new(format!(
                    "the table {:?} was dumped twice, so the output was misread",
                    name.as_str()
                )));
            }
        }

        Ok(Self(filed))
    }

    pub fn tables(&self) -> &BTreeMap<TableName, BTreeMap<ChainName, FirewallChain>> {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<&Ruleset> for Observation {
    fn from(ruleset: &Ruleset) -> Self {
        Observation::object(ruleset.tables().iter().map(|(table, chains)| {
            let chains = Observation::object(
                chains
                    .iter()
                    .map(|(chain, rules)| (chain.as_str(), Observation::from(rules))),
            );

            (table.as_str(), chains)
        }))
    }
}
