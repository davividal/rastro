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
    AccessRule, Authentication, AuthorisedUser, Binary, Certificate, CertificateDetails,
    CertificateReading, Configuration, ConfigurationFile, Directive, KeyFile, KeyReading, Listen,
    Location, Master, PassTarget, Upstream, UpstreamServer, VirtualHost, WebServer,
};
pub use source::{
    ConfigurationFiles, NginxBinary, certificate_file, conf_syntax, htpasswd, master_process,
    nginx_binary, nginx_directives,
};
pub use value_objects::{
    AddressPattern, BuildVersion, ConfigurationSource, ConfigureArgument, DirectiveArgument,
    DirectiveName, Endpoint, FileReading, ListenOption, LocationPattern, PassKind, PasswordScheme,
    Permission, SecondsSinceEpoch, ServerName, ServerParameter, UpstreamName,
};

use std::path::{Path, PathBuf};

use rastro_collector::{
    CollectionError, Collector, CollectorCategory, CollectorId, CollectorIdentity,
    CollectorVersion, FacetName, FilesystemClaim, Observation, Presence, WalkedTree,
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

impl NginxCollector {
    /// One reading of the binary, the running master and the configuration on disk.
    ///
    /// Shared by [`Collector::collect`] and [`Collector::filesystem_claims`] so that the two
    /// cannot disagree about which file is the configuration — which they would, if one
    /// followed the running master's `-c` and the other did not.
    fn read(&self) -> Result<ServerReading, CollectionError> {
        let source = self.binary.as_ref().ok_or_else(|| {
            CollectionError::new(
                "no nginx was found in a system directory, so there is none to ask",
            )
        })?;

        let binary = source.read()?;
        let master = master_process::find(Path::new(binary.path.as_str()))?;

        // What the running server was told beats what the binary was built with, because a
        // master started with `-c` is reading a different file from the one nginx defaults
        // to, and describing the default would describe a service nobody is running.
        let running = master.as_ref();
        let told_root = running.and_then(|master| master.configuration_path.as_ref());
        let told_prefix = running.and_then(|master| master.prefix.as_ref());

        let chosen_by = match told_root {
            Some(_) => ConfigurationSource::RunningMaster,
            None => ConfigurationSource::CompiledIn,
        };
        let root = told_root.map_or_else(
            || binary.configuration_path(),
            |path| path.as_str().to_owned(),
        );
        let prefix =
            told_prefix.map_or_else(|| binary.prefix(), |prefix| prefix.as_str().to_owned());
        let configuration = ConfigurationFiles::at(root, prefix.clone(), chosen_by)?.read();

        Ok(ServerReading {
            binary,
            master,
            configuration,
            prefix: PathBuf::from(prefix),
        })
    }
}

/// What one reading of this box's nginx produced.
struct ServerReading {
    binary: Binary,
    master: Option<Master>,
    configuration: Configuration,
    /// What relative paths in the configuration resolve against.
    prefix: PathBuf,
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
        let reading = self.read()?;

        Ok(Observation::from(&WebServer {
            hosts: nginx_directives::virtual_hosts(
                &reading.configuration.directives,
                &reading.prefix,
            )?,
            upstreams: nginx_directives::upstreams(&reading.configuration.directives)?,
            binary: reading.binary,
            master: reading.master,
            configuration: reading.configuration,
        }))
    }

    /// The trees nginx writes into for its own purposes, sealed.
    ///
    /// **Sealed rather than merely unhashed**, which is the strongest claim in the
    /// vocabulary and is the same call the postgresql facet makes about a cluster's data
    /// directory. A `proxy_cache_path` on a busy server is tens of thousands of files that
    /// nginx creates, renames and unlinks on its own schedule: walking them reports change on
    /// every run for reasons nobody caused, and the entries are nginx's bookkeeping rather
    /// than anything an operator put there. The root entry stays, so a reader still sees the
    /// directory, its mode and its owner, and the effective table in the `invocation` facet
    /// names this facet as the reason nothing is under it.
    ///
    /// Both halves are claimed: the trees the configuration names, and the ones the binary
    /// was built with, since a box that overrides none of the latter still has five of them.
    ///
    /// **This reads the configuration a second time**, because claims are gathered before any
    /// collector runs and the walk has to know where not to go before it starts. A
    /// configuration edited between the two readings would be claimed as it was and reported
    /// as it became, which is the narrower of the two wrong answers: the alternative is a
    /// walk that hashes a cache because the claim came from a stale read.
    fn filesystem_claims(&self) -> Vec<FilesystemClaim> {
        let Ok(reading) = self.read() else {
            return Vec::new();
        };

        let mut trees =
            nginx_directives::working_trees(&reading.configuration.directives, &reading.prefix);
        trees.extend(reading.binary.working_trees());
        trees.sort();
        trees.dedup();

        trees
            .into_iter()
            .filter_map(|tree| WalkedTree::new(tree).ok())
            .map(FilesystemClaim::sealed)
            .collect()
    }
}
