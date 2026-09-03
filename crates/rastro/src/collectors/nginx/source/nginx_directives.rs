//! nginx's directive vocabulary: which directive means what.
//!
//! The grammar in [`conf_syntax`](super::conf_syntax) knows that a configuration is made of
//! names, arguments and blocks. This is where a `server` block becomes a virtual host and a
//! `listen` becomes an address, which is nginx's own semantics and therefore a host
//! interface like any other.
//!
//! **Nothing is inherited and nothing is resolved.** A `root` in the `http` block is not
//! copied into the hosts below it, a `proxy_pass` naming a pool is not linked to the
//! `upstream` that defines it, and a variable is left as a variable. Each of those is a rule
//! about how nginx *behaves*, and asserting it from the outside would put a conclusion in the
//! document where an observation belongs. What the model does not name, the file digests
//! still cover.
//!
//! **Order is kept where nginx reads it and sorted where it does not.** Access rules are
//! applied first-match, so their order is state; locations are matched in order, so theirs
//! is too. A host's `server_name` entries, its listen addresses and a pool's members are all
//! sets to nginx, so they are sorted and an operator rearranging them reads as no change at
//! all.

use std::collections::BTreeMap;
use std::path::Path;

use rastro_collector::{AbsolutePath, CollectionError, NonEmptyText};

use super::certificate_file;
use super::htpasswd;
use crate::collectors::nginx::model::{
    AccessRule, Authentication, Certificate, CertificateReading, Directive, HttpService, KeyFile,
    Listen, Location, LogDestination, PassTarget, StreamServer, StreamService, Upstream,
    UpstreamServer, VirtualHost,
};
use crate::collectors::nginx::value_objects::{
    AddressPattern, Endpoint, ListenOption, LocationPattern, LogKind, PassKind, Permission,
    ServerName, ServerParameter, UpstreamName,
};

const HTTP: &str = "http";
const STREAM: &str = "stream";
const SERVER: &str = "server";
const UPSTREAM: &str = "upstream";
const LOCATION: &str = "location";
const LISTEN: &str = "listen";
const SERVER_NAME: &str = "server_name";
const ROOT: &str = "root";
const SSL_CERTIFICATE: &str = "ssl_certificate";
const SSL_CERTIFICATE_KEY: &str = "ssl_certificate_key";
const AUTH_BASIC: &str = "auth_basic";
const AUTH_BASIC_USER_FILE: &str = "auth_basic_user_file";
const SET_REAL_IP_FROM: &str = "set_real_ip_from";
const RESOLVER: &str = "resolver";

/// What separates a `resolver`'s settings from its addresses: `valid=30s`, `ipv6=off`.
const SETTING_SEPARATOR: char = '=';

/// What makes a path something nginx only resolves when a request arrives.
const VARIABLE: char = '$';

/// nginx's way of writing a certificate into the configuration instead of a path to one.
const INLINE: &str = "data:";

/// The two directive suffixes that name a tree nginx writes into for its own purposes.
const CACHE_PATH: &str = "_cache_path";
const TEMP_PATH: &str = "_temp_path";

/// The trees nginx keeps its own working files in, resolved against the prefix.
///
/// **A cache is the one tree on a web server where the walk's agnosticism turns against
/// it.** `proxy_cache_path` is tens of thousands of files that nginx creates, renames and
/// unlinks on its own schedule, so a walk of it reports change on every run for reasons that
/// have nothing to do with anybody changing the box. The temp paths are the same story with
/// fewer files.
///
/// Found by suffix rather than by a list of directive names, because every protocol nginx
/// proxies brings its own pair — `proxy_`, `fastcgi_`, `uwsgi_`, `scgi_`, `grpc_` — and a
/// list would go quiet about the next one.
pub fn working_trees(directives: &[Directive], prefix: &Path) -> Vec<String> {
    let mut found = Vec::new();
    collect_working_trees(directives, prefix, &mut found);
    found.sort();
    found.dedup();
    found
}

fn collect_working_trees(directives: &[Directive], prefix: &Path, found: &mut Vec<String>) {
    for directive in directives {
        let name = directive.name.as_str();

        let names_a_tree = name.ends_with(CACHE_PATH) || name.ends_with(TEMP_PATH);
        if let Some(path) = directive.arguments.first().filter(|_| names_a_tree) {
            found.push(prefix.join(path.as_str()).to_string_lossy().into_owned());
        }

        if let Some(block) = &directive.block {
            collect_working_trees(block, prefix, found);
        }
    }
}

/// What the `http` context declares: its virtual hosts, in written order, and its pools.
pub fn http_service(
    directives: &[Directive],
    configuration_prefix: &Path,
) -> Result<HttpService, CollectionError> {
    let mut hosts = Vec::new();

    for server in blocks_of(directives, HTTP, SERVER) {
        hosts.push(virtual_host(server, configuration_prefix)?);
    }

    Ok(HttpService {
        hosts,
        upstreams: upstreams_of(directives, HTTP)?,
    })
}

/// What the `stream` context declares: its servers, in written order, and its pools.
pub fn stream_service(
    directives: &[Directive],
    configuration_prefix: &Path,
) -> Result<StreamService, CollectionError> {
    let mut servers = Vec::new();

    for server in blocks_of(directives, STREAM, SERVER) {
        servers.push(stream_server(server, configuration_prefix)?);
    }

    Ok(StreamService {
        servers,
        upstreams: upstreams_of(directives, STREAM)?,
    })
}

/// Every pool one context declares, sorted by name.
fn upstreams_of(directives: &[Directive], context: &str) -> Result<Vec<Upstream>, CollectionError> {
    let mut pools = Vec::new();

    for directive in inside(directives, context) {
        if directive.name.as_str() != UPSTREAM {
            continue;
        }

        let Some(block) = &directive.block else {
            continue;
        };

        pools.push(upstream(directive, block)?);
    }

    pools.sort_by(|left, right| left.name.as_str().cmp(right.name.as_str()));
    Ok(pools)
}

/// The directives inside every block of one context.
///
/// `http` and `stream` are read separately rather than together: the same port number means
/// different things in each, and a server that moved from one to the other has changed what
/// it does with every connection.
fn inside<'a>(
    directives: &'a [Directive],
    context: &'a str,
) -> impl Iterator<Item = &'a Directive> {
    directives
        .iter()
        .filter(move |directive| directive.name.as_str() == context)
        .filter_map(|block| block.block.as_deref())
        .flatten()
}

fn blocks_of<'a>(
    directives: &'a [Directive],
    context: &'a str,
    name: &'a str,
) -> impl Iterator<Item = &'a [Directive]> {
    inside(directives, context)
        .filter(move |directive| directive.name.as_str() == name)
        .filter_map(|directive| directive.block.as_deref())
}

/// One `stream` server, which has no names and no locations to have.
fn stream_server(
    block: &[Directive],
    configuration_prefix: &Path,
) -> Result<StreamServer, CollectionError> {
    let mut server = StreamServer {
        listens: Vec::new(),
        pass: None,
        certificates: Vec::new(),
        access: Vec::new(),
        logs: Vec::new(),
    };
    let mut certificates = Vec::new();
    let mut keys = Vec::new();

    for directive in block {
        let name = directive.name.as_str();

        if let Some(kind) = PassKind::of(name) {
            server.pass = Some(PassTarget {
                kind,
                target: first(directive, "a target")?,
            });
            continue;
        }

        match name {
            LISTEN => server.listens.push(listen(directive)?),
            SSL_CERTIFICATE => certificates.push(first(directive, "a certificate path")?),
            SSL_CERTIFICATE_KEY => keys.push(first(directive, "a key path")?),
            _ => {
                if let Some(rule) = access_rule(name, directive)? {
                    server.access.push(rule);
                }
                if let Some(log) = log_destination(name, directive)? {
                    server.logs.push(log);
                }
            }
        }
    }

    server.certificates = paired(certificates, keys, configuration_prefix);
    server.listens.sort();
    server.logs.sort();

    Ok(server)
}

fn virtual_host(
    block: &[Directive],
    configuration_prefix: &Path,
) -> Result<VirtualHost, CollectionError> {
    let mut host = VirtualHost {
        listens: Vec::new(),
        server_names: Vec::new(),
        root: None,
        certificates: Vec::new(),
        access: Vec::new(),
        logs: Vec::new(),
        authentication: None,
        trusted_proxies: Vec::new(),
        resolvers: Vec::new(),
        locations: Vec::new(),
    };
    let mut certificates = Vec::new();
    let mut keys = Vec::new();
    let mut realm = None;
    let mut user_file = None;

    for directive in block {
        match directive.name.as_str() {
            LISTEN => host.listens.push(listen(directive)?),
            SERVER_NAME => {
                for argument in &directive.arguments {
                    host.server_names.push(ServerName::new(argument.as_str())?);
                }
            }
            ROOT => host.root = Some(first(directive, "a directory")?),
            SSL_CERTIFICATE => certificates.push(first(directive, "a certificate path")?),
            SSL_CERTIFICATE_KEY => keys.push(first(directive, "a key path")?),
            SET_REAL_IP_FROM => host.trusted_proxies.push(address(directive)?),
            RESOLVER => host.resolvers.extend(addresses(directive)?),
            AUTH_BASIC => realm = Some(first(directive, "a realm")?),
            AUTH_BASIC_USER_FILE => {
                user_file = Some(resolved(
                    &first(directive, "a user file")?,
                    configuration_prefix,
                )?);
            }
            LOCATION => {
                if let Some(inside) = &directive.block {
                    host.locations
                        .push(location(directive, inside, configuration_prefix)?);
                }
            }
            name => {
                if let Some(rule) = access_rule(name, directive)? {
                    host.access.push(rule);
                }
                if let Some(log) = log_destination(name, directive)? {
                    host.logs.push(log);
                }
            }
        }
    }

    host.certificates = paired(certificates, keys, configuration_prefix);
    host.authentication = authentication(realm, user_file)?;
    host.listens.sort();
    host.logs.sort();
    host.server_names.sort();
    host.trusted_proxies.sort();
    host.resolvers.sort();

    Ok(host)
}

fn location(
    directive: &Directive,
    block: &[Directive],
    configuration_prefix: &Path,
) -> Result<Location, CollectionError> {
    let pattern = directive
        .arguments
        .iter()
        .map(|argument| argument.as_str())
        .collect::<Vec<&str>>()
        .join(" ");

    let mut location = Location {
        pattern: LocationPattern::new(pattern)?,
        pass: None,
        root: None,
        access: Vec::new(),
        logs: Vec::new(),
        authentication: None,
        locations: Vec::new(),
    };
    let mut realm = None;
    let mut user_file = None;

    for inside in block {
        let name = inside.name.as_str();

        if let Some(kind) = PassKind::of(name) {
            location.pass = Some(PassTarget {
                kind,
                target: first(inside, "a target")?,
            });
            continue;
        }

        match name {
            ROOT => location.root = Some(first(inside, "a directory")?),
            AUTH_BASIC => realm = Some(first(inside, "a realm")?),
            AUTH_BASIC_USER_FILE => {
                user_file = Some(resolved(
                    &first(inside, "a user file")?,
                    configuration_prefix,
                )?);
            }
            LOCATION => {
                if let Some(nested) = &inside.block {
                    location
                        .locations
                        .push(self::location(inside, nested, configuration_prefix)?);
                }
            }
            _ => {
                if let Some(rule) = access_rule(name, inside)? {
                    location.access.push(rule);
                }
                if let Some(log) = log_destination(name, inside)? {
                    location.logs.push(log);
                }
            }
        }
    }

    location.logs.sort();
    location.authentication = authentication(realm, user_file)?;
    Ok(location)
}

fn upstream(directive: &Directive, block: &[Directive]) -> Result<Upstream, CollectionError> {
    let mut servers = Vec::new();
    let mut settings = BTreeMap::new();

    for inside in block {
        if inside.name.as_str() == SERVER {
            servers.push(UpstreamServer {
                endpoint: Endpoint::new(first(inside, "an address")?.as_str())?,
                parameters: sorted_parameters(inside)?,
            });
            continue;
        }

        let arguments = inside
            .arguments
            .iter()
            .map(|argument| argument.as_str())
            .collect::<Vec<&str>>()
            .join(" ");
        settings.insert(inside.name.as_str().to_owned(), arguments);
    }

    servers.sort();

    Ok(Upstream {
        name: UpstreamName::new(first(directive, "a name")?.as_str())?,
        servers,
        settings,
    })
}

fn listen(directive: &Directive) -> Result<Listen, CollectionError> {
    let mut options = Vec::new();
    for argument in directive.arguments.iter().skip(1) {
        options.push(ListenOption::new(argument.as_str())?);
    }
    options.sort();

    Ok(Listen {
        endpoint: Endpoint::new(first(directive, "an address")?.as_str())?,
        options,
    })
}

fn access_rule(name: &str, directive: &Directive) -> Result<Option<AccessRule>, CollectionError> {
    let Some(permission) = Permission::of(name) else {
        return Ok(None);
    };

    Ok(Some(AccessRule {
        permission,
        subject: address(directive)?,
    }))
}

/// One `access_log` or `error_log`, when the directive is one.
///
/// The first argument says where — a path, `off`, or a `syslog:` destination — and the rest
/// says how: an access log's format name and buffering, an error log's level. The rest is
/// kept whole rather than taken apart, because the two directives spell it differently and
/// neither spelling is state a reader compares field by field.
fn log_destination(
    name: &str,
    directive: &Directive,
) -> Result<Option<LogDestination>, CollectionError> {
    let Some(kind) = LogKind::of(name) else {
        return Ok(None);
    };

    let detail = directive
        .arguments
        .iter()
        .skip(1)
        .map(|argument| argument.as_str())
        .collect::<Vec<&str>>()
        .join(" ");

    Ok(Some(LogDestination {
        kind,
        target: first(directive, "a destination")?,
        detail: match detail.is_empty() {
            true => None,
            false => Some(NonEmptyText::new(detail, "log detail")?),
        },
    }))
}

fn address(directive: &Directive) -> Result<AddressPattern, CollectionError> {
    AddressPattern::new(first(directive, "an address")?.as_str())
}

/// The addresses a directive names, leaving its `name=value` settings out.
fn addresses(directive: &Directive) -> Result<Vec<AddressPattern>, CollectionError> {
    directive
        .arguments
        .iter()
        .map(|argument| argument.as_str())
        .filter(|argument| !argument.contains(SETTING_SEPARATOR))
        .map(AddressPattern::new)
        .collect()
}

fn sorted_parameters(directive: &Directive) -> Result<Vec<ServerParameter>, CollectionError> {
    let mut parameters = Vec::new();
    for argument in directive.arguments.iter().skip(1) {
        parameters.push(ServerParameter::new(argument.as_str())?);
    }
    parameters.sort();

    Ok(parameters)
}

/// The first argument of a directive that must have one.
fn first(directive: &Directive, expected: &str) -> Result<NonEmptyText, CollectionError> {
    let argument = directive.arguments.first().ok_or_else(|| {
        CollectionError::new(format!(
            "{} was given no arguments, and it takes {expected}",
            directive.name.as_str()
        ))
    })?;

    NonEmptyText::new(argument.as_str(), directive.name.as_str())
}

/// A path as nginx would open it: relative to the configuration_prefix unless it is already absolute.
fn resolved(
    path: &NonEmptyText,
    configuration_prefix: &Path,
) -> Result<AbsolutePath, CollectionError> {
    let joined = configuration_prefix.join(path.as_str());

    AbsolutePath::new(joined.to_string_lossy(), "nginx file path")
}

/// Certificates and keys, paired the way nginx pairs them, and each read where it can be.
fn paired(
    certificates: Vec<NonEmptyText>,
    keys: Vec<NonEmptyText>,
    configuration_prefix: &Path,
) -> Vec<Certificate> {
    let mut keys = keys.into_iter();

    certificates
        .into_iter()
        .map(|certificate| {
            let key = keys.next();

            Certificate {
                reading: certificate_reading(&certificate, configuration_prefix),
                key_file: key
                    .as_ref()
                    .and_then(|key| key_file(key, configuration_prefix)),
                certificate,
                key,
            }
        })
        .collect()
}

fn certificate_reading(written: &NonEmptyText, configuration_prefix: &Path) -> CertificateReading {
    let refused = |reason: String| CertificateReading::Refused {
        reason: NonEmptyText::new(reason, "certificate refusal")
            .expect("every reason below says something"),
    };

    match on_disk(written, configuration_prefix) {
        Err(reason) => refused(reason),
        Ok(path) => match certificate_file::read(Path::new(path.as_str())) {
            Ok(details) => CertificateReading::Parsed(Box::new(details)),
            Err(error) => refused(error.to_string()),
        },
    }
}

/// The key file described, when the configuration names one that is a path at all.
///
/// A key named through a variable leaves this empty, and the written value stays in the
/// certificate's `key` where a reader can see what was asked for.
fn key_file(written: &NonEmptyText, configuration_prefix: &Path) -> Option<KeyFile> {
    on_disk(written, configuration_prefix)
        .ok()
        .map(|path| certificate_file::describe_key(&path))
}

/// A written certificate or key path as a file on this box, or why it is not one.
///
/// Two of nginx's spellings name no file at all: a path holding a variable is only resolved
/// when a request arrives, and `data:` carries the certificate inline. Neither is a failure,
/// and reporting either as an unreadable file would be wrong about the host.
fn on_disk(written: &NonEmptyText, configuration_prefix: &Path) -> Result<AbsolutePath, String> {
    if written.as_str().contains(VARIABLE) {
        return Err(format!(
            "{:?} names a variable, which nginx resolves per request rather than per file",
            written.as_str()
        ));
    }

    if written.as_str().starts_with(INLINE) {
        return Err(format!(
            "{:?} holds the certificate inline rather than naming a file",
            written.as_str()
        ));
    }

    resolved(written, configuration_prefix).map_err(|error| error.to_string())
}

/// The wall in front of a host or a location, when either half of one was declared.
fn authentication(
    realm: Option<NonEmptyText>,
    user_file: Option<AbsolutePath>,
) -> Result<Option<Authentication>, CollectionError> {
    if realm.is_none() && user_file.is_none() {
        return Ok(None);
    }

    let mut users = Vec::new();
    let mut refusal = None;

    if let Some(file) = &user_file {
        match htpasswd::read(Path::new(file.as_str())) {
            Ok(read) => users = read,
            Err(error) => {
                refusal = Some(NonEmptyText::new(error.to_string(), "user file refusal")?);
            }
        }
    }

    Ok(Some(Authentication {
        realm,
        user_file,
        users,
        refusal,
    }))
}
