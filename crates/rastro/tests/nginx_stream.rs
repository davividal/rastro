//! The `stream` context: TCP and UDP proxied without being read as HTTP.
//!
//! A `stream` server has no `server_name` and no locations, so it is a different shape from
//! a virtual host rather than a poorer one. What it shares — where it listens, what it serves
//! TLS with, who may reach it, where it logs — is the same model, so the two can be compared.

use std::path::PathBuf;

mod support;

use rastro::collectors::nginx::source::ConfigurationFiles;
use rastro::collectors::nginx::value_objects::{
    ConfigurationSource, Endpoint, LogKind, PassKind, Permission,
};
use rastro::collectors::nginx::{HttpService, StreamService, nginx_directives};
use support::fs_tree::{scratch_tree, write};

/// A box that proxies a database port and serves a website, which is the case the two
/// contexts exist to keep apart: both name port 5432 and they mean different things.
const CONFIGURATION: &str = r#"
stream {
    upstream database {
        least_conn;
        server 10.0.0.11:5432 max_fails=3;
        server 10.0.0.12:5432 backup;
    }

    server {
        listen 5432;
        listen 5433 udp;
        proxy_pass database;
        ssl_certificate     /etc/ssl/db.crt;
        ssl_certificate_key /etc/ssl/db.key;
        allow 10.0.0.0/8;
        deny all;
        access_log /var/log/nginx/stream.log basic;
        error_log /var/log/nginx/stream-error.log warn;
    }
}

http {
    server {
        listen 8080;
        server_name example.org;
        access_log /var/log/nginx/access.log combined;
        access_log off;
        error_log /var/log/nginx/error.log;

        location /quiet {
            access_log off;
        }
    }
}
"#;

fn fixture(name: &str) -> PathBuf {
    let prefix = scratch_tree(&format!("nginx-stream-{name}"), &[]);
    write(&prefix, "nginx.conf", CONFIGURATION);
    prefix
}

fn services(name: &str) -> (HttpService, StreamService) {
    let prefix = fixture(name);
    let configuration = ConfigurationFiles::at(
        prefix.join("nginx.conf"),
        &prefix,
        ConfigurationSource::CompiledIn,
    )
    .expect("the scratch tree is absolute")
    .read();

    (
        nginx_directives::http_service(&configuration.directives, &prefix)
            .expect("this configuration holds no directive rastro cannot read"),
        nginx_directives::stream_service(&configuration.directives, &prefix)
            .expect("this configuration holds no directive rastro cannot read"),
    )
}

#[test]
fn a_stream_server_is_not_read_as_a_virtual_host() {
    // Act
    let (http, stream) = services("contexts");

    // Assert: one server in each context, and neither has taken the other's.
    assert_eq!(stream.servers.len(), 1);
    assert_eq!(http.hosts.len(), 1);
    assert_eq!(
        http.hosts[0]
            .server_names
            .iter()
            .map(|name| name.as_str())
            .collect::<Vec<&str>>(),
        ["example.org"]
    );
}

#[test]
fn a_stream_server_keeps_where_it_listens_and_where_it_sends() {
    // Act
    let (_, stream) = services("listen");
    let server = &stream.servers[0];

    // Assert: `udp` is an ordinary listen option, so a port that changed protocol shows.
    assert_eq!(server.listens.len(), 2);
    assert!(matches!(
        &server.listens[0].endpoint,
        Endpoint::Inet { host: None, port: Some(port) } if port.as_u16() == 5432
    ));
    assert_eq!(
        server.listens[1]
            .options
            .iter()
            .map(|option| option.as_str())
            .collect::<Vec<&str>>(),
        ["udp"]
    );

    let pass = server.pass.as_ref().expect("this server proxies");
    assert_eq!(pass.kind, PassKind::Proxy);
    assert_eq!(pass.target.as_str(), "database");
}

#[test]
fn a_stream_server_carries_its_certificate_and_its_access_rules() {
    // Act
    let (_, stream) = services("guards");
    let server = &stream.servers[0];

    // Assert
    assert_eq!(server.certificates.len(), 1);
    assert_eq!(
        server.certificates[0].certificate.as_str(),
        "/etc/ssl/db.crt"
    );
    assert_eq!(
        server
            .access
            .iter()
            .map(|rule| (rule.permission, rule.subject.as_str()))
            .collect::<Vec<(Permission, &str)>>(),
        [(Permission::Allow, "10.0.0.0/8"), (Permission::Deny, "all")]
    );
}

#[test]
fn a_stream_context_has_pools_of_its_own() {
    // Arrange: an `upstream` is spelled the same in both contexts, so it is one model. Which
    // context it belongs to is not.
    // Act
    let (http, stream) = services("pools");

    // Assert
    assert!(http.upstreams.is_empty());
    assert_eq!(stream.upstreams.len(), 1);
    assert_eq!(stream.upstreams[0].name.as_str(), "database");
    assert_eq!(stream.upstreams[0].servers.len(), 2);
}

#[test]
fn a_log_destination_carries_where_and_how() {
    // Act
    let (_, stream) = services("logs-stream");
    let logs: Vec<(LogKind, &str, Option<&str>)> = stream.servers[0]
        .logs
        .iter()
        .map(|log| {
            (
                log.kind,
                log.target.as_str(),
                log.detail.as_ref().map(|detail| detail.as_str()),
            )
        })
        .collect();

    // Assert: sorted, because a block may declare several and nginx writes to all of them.
    assert_eq!(
        logs,
        [
            (LogKind::Access, "/var/log/nginx/stream.log", Some("basic")),
            (
                LogKind::Error,
                "/var/log/nginx/stream-error.log",
                Some("warn")
            ),
        ]
    );
}

#[test]
fn a_log_switched_off_is_a_destination_like_any_other() {
    // Arrange: `access_log off;` is how a location stops being logged, and nothing else in a
    // fingerprint would say that it had.
    // Act
    let (http, _) = services("logs-off");
    let host = &http.hosts[0];

    // Assert
    let targets: Vec<&str> = host.logs.iter().map(|log| log.target.as_str()).collect();
    assert_eq!(
        targets,
        [
            "/var/log/nginx/access.log",
            "off",
            "/var/log/nginx/error.log"
        ]
    );
    assert_eq!(host.locations[0].logs.len(), 1);
    assert_eq!(host.locations[0].logs[0].target.as_str(), "off");
}
