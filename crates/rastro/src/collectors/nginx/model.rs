//! What an nginx configuration is, in rastro's terms rather than in nginx's spelling.

mod access_rule;
mod authentication;
mod authorised_user;
mod binary;
mod certificate;
mod configuration;
mod configuration_file;
mod directive;
mod listen;
mod location;
mod pass_target;
mod upstream;
mod upstream_server;
mod virtual_host;
mod web_server;

pub use access_rule::AccessRule;
pub use authentication::Authentication;
pub use authorised_user::AuthorisedUser;
pub use binary::Binary;
pub use certificate::Certificate;
pub use configuration::Configuration;
pub use configuration_file::ConfigurationFile;
pub use directive::Directive;
pub use listen::Listen;
pub use location::Location;
pub use pass_target::PassTarget;
pub use upstream::Upstream;
pub use upstream_server::UpstreamServer;
pub use virtual_host::VirtualHost;
pub use web_server::WebServer;
