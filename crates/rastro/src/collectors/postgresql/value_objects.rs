//! The typed fields a server setting is made of.
//!
//! Each renders as a leaf of the facet. Nothing here knows that psql exists, which is what
//! keeps the source replaceable: a libpq connection would report the same concepts.

mod cluster_id;
mod cluster_status;
mod database_name;
mod database_privilege;
mod extension_name;
mod grantee;
mod password_method;
mod role_name;
mod setting_name;
mod setting_source;
mod setting_unit;
mod setting_value;

pub use cluster_id::ClusterId;
pub use cluster_status::ClusterStatus;
pub use database_name::DatabaseName;
pub use database_privilege::DatabasePrivilege;
pub use extension_name::ExtensionName;
pub use grantee::Grantee;
pub use password_method::PasswordMethod;
pub use role_name::RoleName;
pub use setting_name::SettingName;
pub use setting_source::SettingSource;
pub use setting_unit::SettingUnit;
pub use setting_value::SettingValue;
