//! How a cluster is read: one module per host interface.

mod cluster_inventory;
mod postgresql_clusters;
mod psql_database_grants;
mod psql_databases;
mod psql_extensions;
mod psql_memberships;
mod psql_result_set;
mod psql_roles;
mod psql_settings;

pub use cluster_inventory::{ClusterInventory, RegisteredCluster};
pub use postgresql_clusters::PostgresqlClusters;
pub use psql_database_grants::PsqlDatabaseGrants;
pub use psql_databases::PsqlDatabases;
pub use psql_extensions::PsqlExtensions;
pub use psql_memberships::PsqlMemberships;
pub use psql_result_set::PsqlResultSet;
pub use psql_roles::PsqlRoles;
pub use psql_settings::PsqlSettings;
