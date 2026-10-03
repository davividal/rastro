//! What happens to data over time: lifecycle policies and snapshot repositories.

use std::collections::BTreeMap;

use rastro_collector::Observation;

use crate::collectors::elasticsearch::value_objects::ApiValue;

/// ILM policies by name.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IlmPolicies(pub BTreeMap<String, IlmPolicy>);

/// One policy: its phases and the version the node numbers every edit with.
///
/// `in_use_by` is left out, because it names the indices under the policy and those rotate.
/// So is `modified_date`, since `version` already moves on every edit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IlmPolicy {
    pub version: Option<i64>,
    pub policy: ApiValue,
}

/// Snapshot repositories by name.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SnapshotRepositories(pub BTreeMap<String, SnapshotRepository>);

/// Where snapshots go.
///
/// **The settings are sensitive, whole.** An S3 or Azure repository that was not set up
/// through the keystore carries its credentials here, and which keys hold one differs by
/// repository type and plugin version, so no one key is trusted by name, the rule the RabbitMQ
/// facet applies to a runtime parameter. The type stays public: it is what says the backups
/// went somewhere else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotRepository {
    pub repository_type: String,
    pub settings: ApiValue,
}

impl From<&IlmPolicies> for Observation {
    fn from(policies: &IlmPolicies) -> Self {
        Observation::object(policies.0.iter().map(|(name, policy)| {
            (
                name.as_str(),
                Observation::object([
                    (
                        "version",
                        match policy.version {
                            Some(version) => Observation::integer(version),
                            None => Observation::null(),
                        },
                    ),
                    ("policy", Observation::from(&policy.policy)),
                ]),
            )
        }))
    }
}

impl From<&SnapshotRepositories> for Observation {
    fn from(repositories: &SnapshotRepositories) -> Self {
        Observation::object(repositories.0.iter().map(|(name, repository)| {
            (
                name.as_str(),
                Observation::object([
                    ("type", Observation::text(&repository.repository_type)),
                    (
                        "settings",
                        Observation::from(&repository.settings).sensitive(),
                    ),
                ]),
            )
        }))
    }
}
