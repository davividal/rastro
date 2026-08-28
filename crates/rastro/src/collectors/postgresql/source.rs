//! How a cluster is read: one module per host interface.

mod cluster_inventory;
mod postgresql_clusters;
mod postmaster_pid;
mod psql_available_extensions;
mod psql_control_data;
mod psql_database_grants;
mod psql_databases;
mod psql_extensions;
mod psql_file_settings;
mod psql_hba_rules;
mod psql_memberships;
mod psql_read_lens;
mod psql_result_set;
mod psql_role_settings;
mod psql_roles;
mod psql_settings;

pub use cluster_inventory::{ClusterInventory, RegisteredCluster};
pub use postgresql_clusters::PostgresqlClusters;
pub use postmaster_pid::PostmasterPid;
pub use psql_available_extensions::PsqlAvailableExtensions;
pub use psql_control_data::PsqlControlData;
pub use psql_database_grants::PsqlDatabaseGrants;
pub use psql_databases::PsqlDatabases;
pub use psql_extensions::PsqlExtensions;
pub use psql_file_settings::PsqlFileSettings;
pub use psql_hba_rules::PsqlHbaRules;
pub use psql_memberships::PsqlMemberships;
pub use psql_read_lens::PsqlReadLens;
pub use psql_result_set::PsqlResultSet;
pub use psql_role_settings::PsqlRoleSettings;
pub use psql_roles::PsqlRoles;
pub use psql_settings::PsqlSettings;
