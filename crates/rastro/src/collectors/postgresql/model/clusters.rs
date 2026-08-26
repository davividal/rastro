//! Every cluster on the box.

use std::collections::BTreeMap;

use rastro_collector::Observation;

use crate::collectors::postgresql::model::Cluster;
use crate::collectors::postgresql::value_objects::ClusterId;

/// The clusters, keyed by `version/name`.
///
/// **Keyed rather than listed, because one box legitimately runs several.** The exporters
/// facet reached the same conclusion from the other side, keying on the unit because "a box
/// with two PostgreSQL clusters runs two `postgres_exporter` instances"; this is that box
/// seen from the database. A `BTreeMap` because the key order is part of the output
/// contract, and [`ClusterId`]'s own ordering puts `9/main` before `16/main`.
///
/// An empty map is a legal, meaningful value: postgresql-common installed with no cluster
/// created is a real state, and different from having no PostgreSQL at all.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Clusters(BTreeMap<ClusterId, Cluster>);

impl Clusters {
    pub fn new(clusters: impl IntoIterator<Item = (ClusterId, Cluster)>) -> Self {
        Self(clusters.into_iter().collect())
    }

    pub fn clusters(&self) -> &BTreeMap<ClusterId, Cluster> {
        &self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<&Clusters> for Observation {
    fn from(clusters: &Clusters) -> Self {
        Observation::object(
            clusters
                .clusters()
                .iter()
                .map(|(id, cluster)| (id.as_str(), Observation::from(cluster))),
        )
    }
}
