//! How a cluster is read: one module per host interface.

mod cluster_inventory;
mod postgresql_clusters;
mod psql_settings;

pub use cluster_inventory::{ClusterInventory, RegisteredCluster};
pub use postgresql_clusters::PostgresqlClusters;
pub use psql_settings::PsqlSettings;
