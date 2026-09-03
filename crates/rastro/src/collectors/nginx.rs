//! Layer 3: what nginx is configured to serve.
//!
//! **Read, never asked.** Every other Layer 3 collector prefers the service's own account
//! of its effective state, because a file says what somebody intended and the server says
//! what it is doing. nginx offers no such account: outside the commercial API it has no
//! runtime introspection at all, and `nginx -T` is not one either — it re-reads the same
//! files from disk and re-resolves the same includes, so a vhost edited without a reload
//! reaches `-T` while the running server carries on with the old one.
//!
//! What `-T` would add over reading the files is include resolution, and it charges for it:
//! testing a configuration opens every log the configuration names, which *creates* the
//! ones that do not exist yet. Measured, on nginx 1.30: a config naming a log file that had
//! never been written left a root-owned empty file behind. A fingerprint tool that does
//! that has changed the box it was called to describe, and the run after it would differ
//! from the run before it for no reason but rastro. So the includes are resolved here
//! instead, the way nginx resolves them, and the resolved file list is recorded so a
//! disagreement is visible rather than silent.
//!
//! That is a narrow licence and not a general one: parse a service's configuration only
//! where the service offers no non-mutating way to report its own effective state, with the
//! measurement attached. See `docs/decisions.md`.

pub mod model;
pub mod source;
pub mod value_objects;

pub use model::{
    AccessRule, Authentication, AuthorisedUser, Binary, Certificate, Configuration,
    ConfigurationFile, Directive, Listen, Location, PassTarget, Upstream, UpstreamServer,
    VirtualHost, WebServer,
};
pub use source::{
    ConfigurationFiles, NginxBinary, conf_syntax, htpasswd, nginx_binary, nginx_directives,
};
pub use value_objects::{
    AddressPattern, BuildVersion, ConfigureArgument, DirectiveArgument, DirectiveName, Endpoint,
    FileReading, ListenOption, LocationPattern, PassKind, PasswordScheme, Permission, ServerName,
    ServerParameter, UpstreamName,
};

use rastro_collector::{
    CollectionError, Collector, CollectorCategory, CollectorId, CollectorIdentity,
    CollectorVersion, FacetName, Observation, Presence,
};

pub struct NginxCollector {
    name: FacetName,
    identity: CollectorIdentity,
    binary: Option<NginxBinary>,
}

impl NginxCollector {
    pub fn new() -> Self {
        Self::reading(NginxBinary::detect())
    }

    /// The same collector over a binary the caller located.
    pub fn reading(binary: Option<NginxBinary>) -> Self {
        Self {
            name: FacetName::new("nginx").expect("`nginx` is a legal facet name"),
            identity: CollectorIdentity::new(
                CollectorId::new("nginx").expect("`nginx` is a legal collector id"),
                CollectorVersion::new("1").expect("`1` is a legal collector version"),
            ),
            binary,
        }
    }
}

impl Default for NginxCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl Collector for NginxCollector {
    fn name(&self) -> &FacetName {
        &self.name
    }

    fn identity(&self) -> &CollectorIdentity {
        &self.identity
    }

    fn category(&self) -> CollectorCategory {
        CollectorCategory::State
    }

    /// `absent` without an nginx binary, `present` with one even if nothing is running.
    ///
    /// The two are different facts and the document keeps them apart, the way the postgresql
    /// facet does: a box with nginx installed and stopped has a configuration somebody wrote
    /// and a service somebody turned off, which is state worth a diff. A box with no nginx at
    /// all serves nothing with it. Neither is a failure, so neither is `Undetermined`; the
    /// reasons rastro cannot look once nginx *is* there reach the document as an `error`
    /// from [`Collector::collect`].
    fn presence(&self) -> Presence {
        match self.binary {
            Some(_) => Presence::Present,
            None => Presence::Absent,
        }
    }

    fn collect(&self) -> Result<Observation, CollectionError> {
        let source = self.binary.as_ref().ok_or_else(|| {
            CollectionError::new(
                "no nginx was found in a system directory, so there is none to ask",
            )
        })?;

        let binary = source.read()?;
        let prefix = binary.prefix();
        let configuration =
            ConfigurationFiles::at(binary.configuration_path(), prefix.clone())?.read();
        let prefix = std::path::Path::new(&prefix);

        Ok(Observation::from(&WebServer {
            hosts: nginx_directives::virtual_hosts(&configuration.directives, prefix)?,
            upstreams: nginx_directives::upstreams(&configuration.directives)?,
            binary,
            configuration,
        }))
    }
}
