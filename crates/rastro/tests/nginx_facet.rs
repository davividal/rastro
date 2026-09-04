//! The shape the facet renders, which is the part of it that is a contract.
//!
//! Every other nginx test asserts what was read; this one asserts what a reader gets. The
//! fixture is built by hand rather than parsed, so that both arms of every variant appear at
//! once: a certificate that read and one that did not, a key described and a key refused, a
//! file digested and a file refused. A facet whose model is right and whose rendering is
//! wrong reads as a wrong host.

mod support;

use rastro::collectors::file_metadata::FileMode;
use rastro::collectors::nginx::model::{
    AccessRule, Authentication, AuthorisedUser, Binary, Certificate, CertificateDetails,
    CertificateReading, Configuration, ConfigurationFile, HttpService, KeyFile, KeyReading, Listen,
    Location, LogDestination, Master, PassTarget, StreamServer, StreamService, Upstream,
    UpstreamServer, VirtualHost, WebServer,
};
use rastro::collectors::nginx::value_objects::{
    AddressPattern, BuildVersion, ConfigurationSource, ConfigureArgument, Endpoint, ListenOption,
    LocationPattern, LogKind, PassKind, PasswordScheme, Permission, SecondsSinceEpoch, ServerName,
    ServerParameter, UpstreamName,
};
use rastro_collector::{AbsolutePath, NonEmptyText, Observation, Xxh3Digest};
use std::collections::BTreeMap;
use support::observation::{field, integer, is_null, items_of, keys_of, text};

fn path(value: &str) -> AbsolutePath {
    AbsolutePath::new(value, "test path").expect("the fixture uses absolute paths")
}

fn words(value: &str) -> NonEmptyText {
    NonEmptyText::new(value, "test value").expect("the fixture uses non-empty values")
}

/// One nginx with every field of the facet filled in, both arms of every variant included.
fn web_server() -> WebServer {
    WebServer {
        binary: Binary {
            path: path("/usr/sbin/nginx"),
            product: words("nginx"),
            version: BuildVersion::new("1.26.3").expect("a version"),
            compiler: None,
            tls_library: Some(words("OpenSSL 3.5.6 7 Apr 2026")),
            configure_arguments: vec![
                ConfigureArgument::new("--prefix=/usr/share/nginx").expect("an argument"),
            ],
        },
        master: Some(Master {
            process_id: 694,
            executable: Some(words("/usr/sbin/nginx (deleted)")),
            started_at: SecondsSinceEpoch::new(1_788_418_716),
            configuration_path: Some(words("/etc/nginx/other.conf")),
            prefix: None,
            worker_count: 7,
            workers_started_at: Some(SecondsSinceEpoch::new(1_788_418_720)),
        }),
        configuration: Configuration {
            prefix: path("/usr/share/nginx"),
            configuration_prefix: path("/etc/nginx"),
            root: path("/etc/nginx/nginx.conf"),
            files: vec![
                ConfigurationFile::parsed(path("/etc/nginx/nginx.conf"), &[]),
                ConfigurationFile::refused(path("/etc/nginx/gone.conf"), words("it is not there")),
            ],
            directives: Vec::new(),
            chosen_by: ConfigurationSource::RunningMaster,
            newest_modified: Some(SecondsSinceEpoch::new(1_788_418_714)),
        },
        http: HttpService {
            hosts: vec![virtual_host()],
            upstreams: vec![upstream()],
        },
        stream: StreamService {
            servers: vec![stream_server()],
            upstreams: vec![upstream()],
        },
    }
}

fn virtual_host() -> VirtualHost {
    VirtualHost {
        listens: vec![Listen {
            endpoint: Endpoint::new("[::]:443").expect("an address"),
            options: vec![ListenOption::new("ssl").expect("an option")],
        }],
        server_names: vec![ServerName::new("example.org").expect("a name")],
        root: Some(words("/srv/www")),
        certificates: vec![read_certificate(), refused_certificate()],
        access: vec![AccessRule {
            permission: Permission::Deny,
            subject: AddressPattern::new("all").expect("a subject"),
        }],
        logs: vec![LogDestination {
            kind: LogKind::Access,
            target: words("/var/log/nginx/access.log"),
            detail: Some(words("combined")),
        }],
        authentication: Some(Authentication {
            realm: Some(words("metrics")),
            user_file: Some(path("/etc/nginx/metrics.htpasswd")),
            users: vec![AuthorisedUser {
                name: words("alice"),
                scheme: PasswordScheme::Bcrypt,
                digest: Some(Xxh3Digest::of(b"a salted verifier")),
            }],
            refusal: None,
        }),
        trusted_proxies: vec![AddressPattern::new("10.0.0.0/8").expect("a proxy")],
        resolvers: vec![AddressPattern::new("10.0.0.53").expect("a resolver")],
        locations: vec![Location {
            pattern: LocationPattern::new("/api/").expect("a pattern"),
            pass: Some(PassTarget {
                kind: PassKind::FastCgi,
                target: words("unix:/run/php.sock"),
            }),
            root: None,
            access: Vec::new(),
            logs: vec![LogDestination {
                kind: LogKind::Error,
                target: words("off"),
                detail: None,
            }],
            authentication: None,
            locations: Vec::new(),
        }],
    }
}

fn read_certificate() -> Certificate {
    Certificate {
        certificate: words("/etc/ssl/example.crt"),
        key: Some(words("/etc/ssl/example.key")),
        reading: CertificateReading::Parsed(Box::new(CertificateDetails {
            subject: words("CN=example.org"),
            issuer: words("CN=example.org"),
            serial: words("90358c17daeeb268"),
            not_before: SecondsSinceEpoch::new(1_788_417_425),
            not_after: SecondsSinceEpoch::new(4_942_017_425),
            subject_alternative_names: vec![words("example.org")],
            key_algorithm: words("rsaEncryption"),
            digest: Xxh3Digest::of(b"the certificate"),
        })),
        key_file: Some(KeyFile {
            path: path("/etc/ssl/example.key"),
            reading: KeyReading::Described {
                mode: FileMode::of(0o100_600),
                owner: 0,
                group: 0,
            },
        }),
    }
}

fn refused_certificate() -> Certificate {
    Certificate {
        certificate: words("/etc/ssl/$ssl_server_name.crt"),
        key: Some(words("/etc/ssl/$ssl_server_name.key")),
        reading: CertificateReading::Refused {
            reason: words("it names a variable"),
        },
        key_file: Some(KeyFile {
            path: path("/etc/ssl/unreadable.key"),
            reading: KeyReading::Refused {
                reason: words("permission denied"),
            },
        }),
    }
}

fn upstream() -> Upstream {
    let mut settings = BTreeMap::new();
    settings.insert("keepalive".to_owned(), "32".to_owned());

    Upstream {
        name: UpstreamName::new("app_pool").expect("a name"),
        servers: vec![UpstreamServer {
            endpoint: Endpoint::new("unix:/run/app.sock").expect("an address"),
            parameters: vec![ServerParameter::new("down").expect("a parameter")],
        }],
        settings,
    }
}

fn stream_server() -> StreamServer {
    StreamServer {
        listens: vec![Listen {
            endpoint: Endpoint::new("5432").expect("an address"),
            options: vec![ListenOption::new("udp").expect("an option")],
        }],
        pass: Some(PassTarget {
            kind: PassKind::Proxy,
            target: words("database"),
        }),
        certificates: vec![read_certificate()],
        access: vec![AccessRule {
            permission: Permission::Allow,
            subject: AddressPattern::new("10.0.0.0/8").expect("a subject"),
        }],
        logs: Vec::new(),
    }
}

fn facet() -> Observation {
    Observation::from(&web_server())
}

#[test]
fn the_facet_holds_the_binary_the_master_the_configuration_and_both_services() {
    // Act & Assert: the top level is the contract a reader navigates by.
    assert_eq!(
        keys_of(&facet()),
        ["binary", "configuration", "http", "master", "stream"]
    );
}

#[test]
fn the_binary_renders_what_it_reported_and_nulls_what_it_did_not() {
    // Act
    let binary = field(&facet(), "binary");

    // Assert: a build that prints no `built by` line renders null rather than vanishing.
    assert_eq!(text(&field(&binary, "version")), "1.26.3");
    assert_eq!(text(&field(&binary, "product")), "nginx");
    assert!(is_null(&field(&binary, "compiler")));
    assert_eq!(
        text(&field(&binary, "tls_library")),
        "OpenSSL 3.5.6 7 Apr 2026"
    );
    assert_eq!(items_of(&field(&binary, "configure_arguments")).len(), 1);
}

#[test]
fn the_master_renders_the_two_instants_that_answer_whether_the_config_is_loaded() {
    // Act
    let master = field(&facet(), "master");

    // Assert: whole seconds, and the deleted marker kept as the kernel wrote it.
    assert_eq!(integer(&field(&master, "process_id")), 694);
    assert_eq!(integer(&field(&master, "started_at")), 1_788_418_716);
    assert_eq!(
        integer(&field(&master, "workers_started_at")),
        1_788_418_720
    );
    assert_eq!(integer(&field(&master, "worker_count")), 7);
    assert_eq!(
        text(&field(&master, "executable")),
        "/usr/sbin/nginx (deleted)"
    );
    assert_eq!(
        text(&field(&master, "configuration_path")),
        "/etc/nginx/other.conf"
    );
    assert!(is_null(&field(&master, "prefix")));
}

#[test]
fn the_configuration_names_both_bases_and_who_chose_the_root() {
    // Act
    let configuration = field(&facet(), "configuration");

    // Assert
    assert_eq!(text(&field(&configuration, "chosen_by")), "running_master");
    assert_eq!(text(&field(&configuration, "prefix")), "/usr/share/nginx");
    assert_eq!(
        text(&field(&configuration, "configuration_prefix")),
        "/etc/nginx"
    );
    assert_eq!(
        integer(&field(&configuration, "newest_modified")),
        1_788_418_714
    );
}

#[test]
fn a_file_renders_its_digest_and_a_refused_one_renders_its_reason() {
    // Arrange
    let files = items_of(&field(&field(&facet(), "configuration"), "files"));

    // Act & Assert: one field, two kinds of answer, so a reader never has to look elsewhere
    // to find out why a file is not digested.
    assert_eq!(text(&field(&files[0], "path")), "/etc/nginx/nginx.conf");
    assert_eq!(text(&field(&files[0], "reading")).len(), 16);
    assert_eq!(text(&field(&files[1], "reading")), "it is not there");
}

#[test]
fn a_host_renders_every_part_of_what_it_serves() {
    // Act
    let host = items_of(&field(&field(&facet(), "http"), "hosts")).remove(0);

    // Assert
    assert_eq!(
        keys_of(&host),
        [
            "access",
            "authentication",
            "certificates",
            "listens",
            "locations",
            "logs",
            "resolvers",
            "root",
            "server_names",
            "trusted_proxies"
        ]
    );

    let listen = items_of(&field(&host, "listens")).remove(0);
    let endpoint = field(&listen, "endpoint");
    assert_eq!(text(&field(&endpoint, "host")), "::");
    assert_eq!(integer(&field(&endpoint, "port")), 443);

    let rule = items_of(&field(&host, "access")).remove(0);
    assert_eq!(text(&field(&rule, "permission")), "deny");
    assert_eq!(text(&field(&rule, "subject")), "all");

    let log = items_of(&field(&host, "logs")).remove(0);
    assert_eq!(text(&field(&log, "kind")), "access_log");
    assert_eq!(text(&field(&log, "detail")), "combined");
}

#[test]
fn a_wall_renders_its_users_and_a_digest_that_is_not_a_password() {
    // Act
    let host = items_of(&field(&field(&facet(), "http"), "hosts")).remove(0);
    let wall = field(&host, "authentication");
    let user = items_of(&field(&wall, "users")).remove(0);

    // Assert: the name is in the document, the verifier never is, and the refusal field is
    // null rather than absent so that "nobody is behind the wall" and "rastro could not
    // look" stay different answers.
    assert_eq!(text(&field(&wall, "realm")), "metrics");
    assert!(is_null(&field(&wall, "refusal")));
    assert_eq!(text(&field(&user, "name")), "alice");
    assert_eq!(text(&field(&user, "scheme")), "bcrypt");
    assert_eq!(text(&field(&user, "digest")).len(), 16);
}

#[test]
fn a_certificate_renders_what_it_says_or_why_it_could_not_be_read() {
    // Act
    let host = items_of(&field(&field(&facet(), "http"), "hosts")).remove(0);
    let certificates = items_of(&field(&host, "certificates"));

    // Assert: the same field carries the reading either way.
    let details = field(&certificates[0], "reading");
    assert_eq!(text(&field(&details, "subject")), "CN=example.org");
    assert_eq!(integer(&field(&details, "not_after")), 4_942_017_425);
    assert_eq!(text(&field(&details, "key_algorithm")), "rsaEncryption");
    assert_eq!(
        items_of(&field(&details, "subject_alternative_names")).len(),
        1
    );
    assert_eq!(
        text(&field(&certificates[1], "reading")),
        "it names a variable"
    );

    // And the key beside it: described, or refused, and never opened either way.
    let described = field(&field(&certificates[0], "key_file"), "reading");
    assert_eq!(text(&field(&described, "mode")), "0600");
    assert_eq!(integer(&field(&described, "owner")), 0);
    assert_eq!(
        text(&field(&field(&certificates[1], "key_file"), "reading")),
        "permission denied"
    );
}

#[test]
fn a_location_renders_where_it_sends_and_what_it_does_not_log() {
    // Act
    let host = items_of(&field(&field(&facet(), "http"), "hosts")).remove(0);
    let location = items_of(&field(&host, "locations")).remove(0);

    // Assert
    assert_eq!(text(&field(&location, "pattern")), "/api/");
    let pass = field(&location, "pass");
    assert_eq!(text(&field(&pass, "kind")), "fastcgi_pass");
    assert_eq!(text(&field(&pass, "target")), "unix:/run/php.sock");
    let log = items_of(&field(&location, "logs")).remove(0);
    assert_eq!(text(&field(&log, "target")), "off");
    assert!(is_null(&field(&log, "detail")));
}

#[test]
fn a_pool_renders_its_members_and_its_settings() {
    // Act
    let pool = items_of(&field(&field(&facet(), "http"), "upstreams")).remove(0);
    let member = items_of(&field(&pool, "servers")).remove(0);

    // Assert
    assert_eq!(text(&field(&pool, "name")), "app_pool");
    assert_eq!(text(&field(&field(&pool, "settings"), "keepalive")), "32");
    assert_eq!(
        text(&field(&field(&member, "endpoint"), "socket")),
        "/run/app.sock"
    );
    assert_eq!(
        text(&items_of(&field(&member, "parameters")).remove(0)),
        "down"
    );
}

#[test]
fn a_stream_server_renders_without_the_fields_it_cannot_have() {
    // Act
    let stream = field(&facet(), "stream");
    let server = items_of(&field(&stream, "servers")).remove(0);

    // Assert: no server_name and no locations, because nginx has no request to name a host
    // with. A port with no address renders the address as null rather than inventing one.
    assert_eq!(
        keys_of(&server),
        ["access", "certificates", "listens", "logs", "pass"]
    );
    let endpoint = field(&items_of(&field(&server, "listens")).remove(0), "endpoint");
    assert!(is_null(&field(&endpoint, "host")));
    assert_eq!(integer(&field(&endpoint, "port")), 5432);
    assert_eq!(items_of(&field(&stream, "upstreams")).len(), 1);
}
