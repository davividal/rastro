//! Which roles a cluster's roles are members of.

use std::collections::BTreeMap;

use rastro_collector::{CollectionError, Observation};

use crate::collectors::postgresql::value_objects::RoleName;

/// One role's membership in another.
///
/// Membership is the widest privilege the tenancy model grants: a member holds everything
/// the granted role owns, future objects included, which is why it is recorded as its own
/// fact rather than folded into either role.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Membership {
    pub member: RoleName,
    pub granted: RoleName,

    /// Whether the member may pass the membership on.
    ///
    /// The difference between inheriting a role and being able to hand it out, which is a
    /// privilege of its own and not implied by the grant.
    pub admin_option: bool,
}

/// Every membership a cluster holds, ordered by member and then by granted role.
///
/// **Empty is an ordinary answer here**, unlike settings and roles: a cluster that grants
/// nothing to anybody is a fresh cluster, not a failed read. Only a repeated grant is
/// refused, because one grant cannot both carry the admin option and not carry it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterMemberships {
    memberships: Vec<Membership>,
}

impl ClusterMemberships {
    pub fn new(mut memberships: Vec<Membership>) -> Result<Self, CollectionError> {
        memberships.sort_by(|left, right| {
            left.member
                .cmp(&right.member)
                .then_with(|| left.granted.cmp(&right.granted))
        });

        if let Some(repeated) = memberships
            .windows(2)
            .find(|pair| pair[0].member == pair[1].member && pair[0].granted == pair[1].granted)
        {
            return Err(CollectionError::new(format!(
                "the server reported {:?} as a member of {:?} twice, so whether the grant carries \
                 the admin option cannot be told",
                repeated[0].member.as_str(),
                repeated[0].granted.as_str()
            )));
        }

        Ok(Self { memberships })
    }

    pub fn memberships(&self) -> &[Membership] {
        &self.memberships
    }
}

impl From<&ClusterMemberships> for Observation {
    /// Grouped by member, because that is the role an operator looks up when asking what
    /// somebody can reach.
    fn from(memberships: &ClusterMemberships) -> Self {
        let mut grouped: BTreeMap<&str, Vec<(&str, Observation)>> = BTreeMap::new();

        for membership in memberships.memberships() {
            grouped
                .entry(membership.member.as_str())
                .or_default()
                .push((
                    membership.granted.as_str(),
                    Observation::object([(
                        "admin_option",
                        Observation::boolean(membership.admin_option),
                    )]),
                ));
        }

        Observation::object(
            grouped
                .into_iter()
                .map(|(member, granted)| (member, Observation::object(granted))),
        )
    }
}
