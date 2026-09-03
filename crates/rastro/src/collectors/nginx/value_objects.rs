//! The typed fields an nginx configuration is made of.
//!
//! Each renders as a leaf of the facet. Nothing here knows how nginx spells a
//! configuration file, which is what keeps the grammar replaceable.

mod address_pattern;
mod build_version;
mod configuration_source;
mod configure_argument;
mod directive_argument;
mod directive_name;
mod endpoint;
mod file_reading;
mod listen_option;
mod location_pattern;
mod pass_kind;
mod password_scheme;
mod permission;
mod seconds_since_epoch;
mod server_name;
mod server_parameter;
mod upstream_name;

pub use address_pattern::AddressPattern;
pub use build_version::BuildVersion;
pub use configuration_source::ConfigurationSource;
pub use configure_argument::ConfigureArgument;
pub use directive_argument::DirectiveArgument;
pub use directive_name::DirectiveName;
pub use endpoint::Endpoint;
pub use file_reading::FileReading;
pub use listen_option::ListenOption;
pub use location_pattern::LocationPattern;
pub use pass_kind::PassKind;
pub use password_scheme::PasswordScheme;
pub use permission::Permission;
pub use seconds_since_epoch::SecondsSinceEpoch;
pub use server_name::ServerName;
pub use server_parameter::ServerParameter;
pub use upstream_name::UpstreamName;
