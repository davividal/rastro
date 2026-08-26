//! What rastro means by a cluster's configuration, as opposed to how psql prints it.

mod cluster_settings;
mod setting;

pub use cluster_settings::ClusterSettings;
pub use setting::Setting;
