//! The typed fields a server setting is made of.
//!
//! Each renders as a leaf of the facet. Nothing here knows that psql exists, which is what
//! keeps the source replaceable: a libpq connection would report the same concepts.

mod cluster_id;
mod cluster_status;
mod setting_name;
mod setting_source;
mod setting_unit;
mod setting_value;

pub use cluster_id::ClusterId;
pub use cluster_status::ClusterStatus;
pub use setting_name::SettingName;
pub use setting_source::SettingSource;
pub use setting_unit::SettingUnit;
pub use setting_value::SettingValue;
