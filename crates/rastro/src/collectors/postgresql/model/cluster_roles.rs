//! Every role a cluster knows.

use rastro_collector::{CollectionError, Observation};

use crate::collectors::postgresql::model::Role;

/// The roles of one cluster, ordered by name.
///
/// Holds the two invariants a single role cannot: each name appears once, and there is at
/// least one. Both refusals exist because the alternative reads as state: a name appearing
/// twice would keep whichever privileges happened to render last, and an empty set would
/// report a cluster as having no roles when every cluster has at least the one that owns it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterRoles {
    roles: Vec<Role>,
}

impl ClusterRoles {
    pub fn new(mut roles: Vec<Role>) -> Result<Self, CollectionError> {
        if roles.is_empty() {
            return Err(CollectionError::new(
                "the server reported no roles at all, and every cluster has at least the role \
                 that owns it",
            ));
        }

        roles.sort_by(|left, right| left.name.cmp(&right.name));

        if let Some(repeated) = roles
            .windows(2)
            .find(|pair| pair[0].name == pair[1].name)
            .map(|pair| pair[0].name.as_str())
        {
            return Err(CollectionError::new(format!(
                "the server reported the role {repeated:?} twice, so which privileges it holds \
                 cannot be told"
            )));
        }

        Ok(Self { roles })
    }

    pub fn roles(&self) -> &[Role] {
        &self.roles
    }
}

impl From<&ClusterRoles> for Observation {
    fn from(roles: &ClusterRoles) -> Self {
        Observation::object(
            roles
                .roles()
                .iter()
                .map(|role| (role.name.as_str(), Observation::from(role))),
        )
    }
}
