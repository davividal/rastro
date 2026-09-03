//! Reading a configuration into the hosts and the pools it declares.
//!
//! The fixture is the shape a real box has: a redirect host on port 80, a serving host on
//! 443 with a certificate and a walled-off metrics location, and a pool with a weighted
//! member, a backup and one taken out of service.

use std::path::{Path, PathBuf};

mod support;

use rastro::collectors::nginx::source::ConfigurationFiles;
use rastro::collectors::nginx::value_objects::{
    ConfigurationSource, Endpoint, PassKind, PasswordScheme, Permission,
};
use rastro::collectors::nginx::{Upstream, VirtualHost, nginx_directives};
use support::fs_tree::{scratch_tree, write};

const CONFIGURATION: &str = r#"
user www-data;
http {
    upstream app_pool {
        least_conn;
        server 10.0.0.8:8080 backup;
        server 10.0.0.7:8080 weight=3 max_fails=2;
        server unix:/run/app.sock down;
        keepalive 32;
    }

    server {
        listen [::]:80 default_server;
        listen 80 default_server;
        server_name www.example.org example.org;
        return 301 https://$host$request_uri;
    }

    server {
        listen 443 ssl http2;
        server_name example.org;
        root /srv/www;
        ssl_certificate     /etc/ssl/example.org.crt;
        ssl_certificate_key /etc/ssl/example.org.key;
        set_real_ip_from 10.0.0.0/8;
        resolver 10.0.0.53 valid=30s;

        location / {
            proxy_pass http://app_pool;
        }

        location /metrics {
            allow 10.0.0.0/8;
            deny all;
            auth_basic "metrics";
            auth_basic_user_file metrics.htpasswd;
        }
    }
}
"#;

/// One salted verifier of each kind rastro digests, and one unsalted one it must not.
const USERS: &str = "\
# the metrics wall
alice:$apr1$Ur6Nn2Cd$0dJH0R3xC5xM0Qb2eYyOM.
bob:{SHA}qUqP5cyxm6YcTAhz05Hph5gvu9M=
carol:$2y$05$Ye7VZ1oQ9dCkK0MTYQ0X1uWJ3G2xkS1uYYD9M8t0LhZlP8H4pJ4Iu
";

fn fixture(name: &str) -> PathBuf {
    let prefix = scratch_tree(&format!("nginx-hosts-{name}"), &[]);
    write(&prefix, "nginx.conf", CONFIGURATION);
    write(&prefix, "metrics.htpasswd", USERS);
    prefix
}

fn hosts_of(prefix: &Path) -> Vec<VirtualHost> {
    let configuration = ConfigurationFiles::at(
        prefix.join("nginx.conf"),
        prefix,
        ConfigurationSource::CompiledIn,
    )
    .expect("the scratch tree is absolute")
    .read();

    nginx_directives::virtual_hosts(&configuration.directives, prefix)
        .expect("this configuration holds no directive rastro cannot read")
}

fn pools_of(prefix: &Path) -> Vec<Upstream> {
    let configuration = ConfigurationFiles::at(
        prefix.join("nginx.conf"),
        prefix,
        ConfigurationSource::CompiledIn,
    )
    .expect("the scratch tree is absolute")
    .read();

    nginx_directives::upstreams(&configuration.directives)
        .expect("this configuration holds no pool rastro cannot read")
}

fn names_of(host: &VirtualHost) -> Vec<&str> {
    host.server_names.iter().map(|name| name.as_str()).collect()
}

#[test]
fn every_server_block_becomes_a_host_in_the_order_it_was_written() {
    // Arrange
    let prefix = fixture("order");

    // Act
    let hosts = hosts_of(&prefix);

    // Assert: order is kept because nginx resolves a default server by it.
    assert_eq!(hosts.len(), 2);
    assert_eq!(names_of(&hosts[0]), ["example.org", "www.example.org"]);
    assert_eq!(names_of(&hosts[1]), ["example.org"]);
}

#[test]
fn a_listen_carries_its_address_and_its_switches() {
    // Arrange
    let prefix = fixture("listen");

    // Act
    let redirect = hosts_of(&prefix).remove(0);

    // Assert: sorted, because nginx reads a host's addresses as a set. The portless one
    // sorts first, and neither is invented: `listen 80` names no address at all.
    assert_eq!(redirect.listens.len(), 2);
    assert!(matches!(
        &redirect.listens[0].endpoint,
        Endpoint::Inet { host: None, port: Some(port) } if port.as_u16() == 80
    ));
    assert!(matches!(
        &redirect.listens[1].endpoint,
        Endpoint::Inet { host: Some(host), port: Some(port) }
            if host.as_str() == "::" && port.as_u16() == 80
    ));
    assert_eq!(
        redirect.listens[0]
            .options
            .iter()
            .map(|option| option.as_str())
            .collect::<Vec<&str>>(),
        ["default_server"]
    );
}

#[test]
fn a_certificate_keeps_the_key_that_belongs_to_it() {
    // Arrange
    let prefix = fixture("certificates");

    // Act
    let served = hosts_of(&prefix).remove(1);

    // Assert
    assert_eq!(served.certificates.len(), 1);
    assert_eq!(
        served.certificates[0].certificate.as_str(),
        "/etc/ssl/example.org.crt"
    );
    assert_eq!(
        served.certificates[0]
            .key
            .as_ref()
            .expect("this host declares a key")
            .as_str(),
        "/etc/ssl/example.org.key"
    );
}

#[test]
fn a_resolver_keeps_its_addresses_and_drops_its_settings() {
    // Arrange: `valid=30s` tunes the cache; it is not a nameserver.
    let prefix = fixture("resolver");

    // Act
    let served = hosts_of(&prefix).remove(1);

    // Assert
    assert_eq!(
        served
            .resolvers
            .iter()
            .map(|resolver| resolver.as_str())
            .collect::<Vec<&str>>(),
        ["10.0.0.53"]
    );
    assert_eq!(
        served
            .trusted_proxies
            .iter()
            .map(|proxy| proxy.as_str())
            .collect::<Vec<&str>>(),
        ["10.0.0.0/8"]
    );
}

#[test]
fn a_location_keeps_where_it_sends_and_who_may_reach_it() {
    // Arrange
    let prefix = fixture("locations");

    // Act
    let served = hosts_of(&prefix).remove(1);

    // Assert: written order, because nginx matches locations in it.
    assert_eq!(served.locations.len(), 2);
    assert_eq!(served.locations[0].pattern.as_str(), "/");
    let pass = served.locations[0]
        .pass
        .as_ref()
        .expect("this location passes the request on");
    assert_eq!(pass.kind, PassKind::Proxy);
    assert_eq!(pass.target.as_str(), "http://app_pool");

    let metrics = &served.locations[1];
    assert_eq!(
        metrics
            .access
            .iter()
            .map(|rule| (rule.permission, rule.subject.as_str()))
            .collect::<Vec<(Permission, &str)>>(),
        [(Permission::Allow, "10.0.0.0/8"), (Permission::Deny, "all")]
    );
}

#[test]
fn a_wall_names_its_users_and_never_their_passwords() {
    // Arrange
    let prefix = fixture("users");

    // Act
    let metrics = hosts_of(&prefix).remove(1).locations.remove(1);
    let wall = metrics
        .authentication
        .expect("this location carries an auth_basic");

    // Assert: a salted verifier is digested so a rotation shows; an unsalted one is not,
    // because a digest of it would be an offline oracle over the password itself.
    assert_eq!(
        wall.realm.expect("a realm was declared").as_str(),
        "metrics"
    );
    assert_eq!(wall.refusal, None);

    let users: Vec<(&str, PasswordScheme, bool)> = wall
        .users
        .iter()
        .map(|user| (user.name.as_str(), user.scheme, user.digest.is_some()))
        .collect();
    assert_eq!(
        users,
        [
            ("alice", PasswordScheme::Apr1, true),
            ("bob", PasswordScheme::Sha1, false),
            ("carol", PasswordScheme::Bcrypt, true),
        ]
    );
}

#[test]
fn a_user_file_that_cannot_be_read_says_so_rather_than_reading_as_empty() {
    // Arrange
    let prefix = scratch_tree("nginx-hosts-no-user-file", &[]);
    write(
        &prefix,
        "nginx.conf",
        "http { server { auth_basic_user_file gone.htpasswd; } }",
    );

    // Act
    let wall = hosts_of(&prefix)
        .remove(0)
        .authentication
        .expect("the directive declares a wall");

    // Assert
    assert!(wall.users.is_empty());
    assert!(
        wall.refusal
            .expect("the file is not there")
            .as_str()
            .contains("could not be read")
    );
}

#[test]
fn a_pool_sorts_its_members_and_keeps_what_each_was_given() {
    // Arrange
    let prefix = fixture("pool");

    // Act
    let pools = pools_of(&prefix);

    // Assert
    assert_eq!(pools.len(), 1);
    let pool = &pools[0];
    assert_eq!(pool.name.as_str(), "app_pool");
    assert_eq!(pool.settings.get("least_conn"), Some(&String::new()));
    assert_eq!(pool.settings.get("keepalive"), Some(&"32".to_owned()));

    // Sorted: the pool is a set, so which line came first is not state.
    let members: Vec<Vec<&str>> = pool
        .servers
        .iter()
        .map(|server| {
            server
                .parameters
                .iter()
                .map(|parameter| parameter.as_str())
                .collect()
        })
        .collect();
    assert_eq!(
        members,
        [
            vec!["max_fails=2", "weight=3"],
            vec!["backup"],
            vec!["down"],
        ]
    );
    assert!(matches!(
        &pool.servers[2].endpoint,
        Endpoint::Unix { path } if path.as_str() == "/run/app.sock"
    ));
}
