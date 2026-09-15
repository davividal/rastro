//! The environment a container was given.

use std::collections::BTreeMap;

use rastro_collector::Observation;

use crate::collectors::containers::value_objects::VariableName;

/// Every variable the container carries, keyed by name, with every value withheld.
///
/// **Every value is sensitive, and this facet does not try to work out which ones are
/// secrets.** The `sysctl` facet judges by key, because the parameters that hold a secret
/// are a closed set somebody can enumerate. An environment is the opposite: it is whatever
/// the operator put there, and the name is a poor witness in both directions.
/// `DSN=postgres://app:s3cret@db/app` carries a credential and matches no keyword a rule
/// could look for, while `MYSQL_ROOT_PASSWORD` announces itself. A rule that guesses fails
/// in the direction that leaks, so there is no rule.
///
/// The names stay public, which is what makes this useful: a diff says `PGPASSWORD` changed,
/// and the digest that says so is not the password. That is the rotation-visible-without-the-
/// secret shape the postgresql facet uses for role passwords.
///
/// **Cost, accepted knowingly:** `PATH` and the rest of the image's benign environment reach
/// the document as digests too, so the complete view reads less well than it could. `--raw`
/// is where that is paid back, once it exists.
///
/// The value is plain text rather than a value object: `--env EMPTY=` is a variable the
/// container has, set to nothing, which is a different fact from not having it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ContainerEnvironment(BTreeMap<VariableName, String>);

impl ContainerEnvironment {
    pub fn new(variables: impl IntoIterator<Item = (VariableName, String)>) -> Self {
        Self(variables.into_iter().collect())
    }

    pub fn variables(&self) -> &BTreeMap<VariableName, String> {
        &self.0
    }
}

impl From<&ContainerEnvironment> for Observation {
    fn from(environment: &ContainerEnvironment) -> Self {
        Observation::object(
            environment
                .variables()
                .iter()
                .map(|(name, value)| (name.as_str(), Observation::text(value).sensitive())),
        )
    }
}
