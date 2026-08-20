//! Every unit the host knows about, from either side.

use std::collections::BTreeMap;

use rastro_collector::Observation;

use super::unit::Unit;
use crate::collectors::systemd::UnitName;

/// The units, keyed by name.
///
/// Keyed rather than listed, as with modules and packages and for the same reason:
/// systemd enforces one unit per name, so keying loses nothing and enabling one service
/// shows as a single changed key rather than as a shifted list.
///
/// A [`BTreeMap`], so ordering is a property of the structure rather than of a `sort`
/// call somebody has to remember. It sorts by the escaped name systemd uses, which is
/// what an operator would sort by too.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UnitRegistry(BTreeMap<UnitName, Unit>);

impl UnitRegistry {
    pub fn new(units: impl IntoIterator<Item = (UnitName, Unit)>) -> Self {
        Self(units.into_iter().collect())
    }

    pub fn units(&self) -> &BTreeMap<UnitName, Unit> {
        &self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<&UnitRegistry> for Observation {
    fn from(registry: &UnitRegistry) -> Self {
        Observation::object(
            registry
                .units()
                .iter()
                .map(|(name, unit)| (name.as_str(), observed(name, unit))),
        )
    }
}

/// One unit, carrying the judgement its name implies.
///
/// A login session's scope is marked volatile whole, rather than field by field,
/// because what churns about it is the *key*: `session-779.scope` becomes
/// `session-780.scope` at the next login. Annotating the entry is what drops the key
/// from the diffable view, since nothing else could.
fn observed(name: &UnitName, unit: &Unit) -> Observation {
    let observation = Observation::from(unit);

    if name.is_a_login_session() {
        return observation.volatile();
    }

    observation
}
