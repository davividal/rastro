//! Every kernel parameter the host reports, as the set it is.

use std::collections::BTreeMap;

use rastro_collector::Observation;

use crate::collectors::sysctl::value_objects::{SysctlKey, SysctlValue};

/// The runtime kernel parameters, keyed by name.
///
/// **A map, not a list, and the reason is the opposite of the mount table's.** A
/// sysctl name is unique by construction: the kernel publishes it at one place in
/// one tree, so nothing can be lost by keying on it, and keying is what removes
/// the walk order of a directory from the output. There are about twelve hundred
/// of these on an ordinary box, and a reader looks one up by name rather than
/// reading the list.
///
/// A [`BTreeMap`] rather than a sorted `Vec` so that the ordering is a property of
/// the structure instead of a `sort` call somebody has to remember. It sorts by
/// the dotted name, which is what an operator scanning a diff would sort by too.
///
/// **The model is genuinely thin here, and that is the honest shape.** There is no
/// per-parameter aggregate wrapping the value, because a parameter *is* a name and
/// a value: the two judgements that would justify a richer type, whether the value
/// moves on its own and whether it is a secret, are properties of the name and
/// live on [`SysctlKey`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SysctlParameters(BTreeMap<SysctlKey, SysctlValue>);

impl SysctlParameters {
    pub fn new(parameters: impl IntoIterator<Item = (SysctlKey, SysctlValue)>) -> Self {
        Self(parameters.into_iter().collect())
    }

    /// The parameters in order, which is the sorted order of their names.
    pub fn iter(&self) -> impl Iterator<Item = (&SysctlKey, &SysctlValue)> {
        self.0.iter()
    }

    pub fn get(&self, name: &str) -> Option<&SysctlValue> {
        self.0
            .iter()
            .find(|(key, _)| key.as_str() == name)
            .map(|(_, value)| value)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<&SysctlParameters> for Observation {
    fn from(parameters: &SysctlParameters) -> Self {
        Observation::object(
            parameters
                .iter()
                .map(|(key, value)| (key.as_str(), observed(key, value))),
        )
    }
}

/// One parameter's value, carrying the judgements its name implies.
///
/// The annotations are attached here rather than inside [`SysctlValue`] because
/// neither is a property of the value: `864` is volatile when it is
/// `fs.file-nr` and stable when it is `net.core.somaxconn`, and the same string
/// is a secret only when it is a fast-open key. Only the pairing knows.
fn observed(key: &SysctlKey, value: &SysctlValue) -> Observation {
    let mut observed = Observation::from(value);

    if key.changes_on_its_own() {
        observed = observed.volatile();
    }
    if key.holds_a_secret() {
        observed = observed.sensitive();
    }

    observed
}
