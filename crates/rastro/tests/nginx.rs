//! The facet itself: what nginx says about its own binary, and how that reaches the
//! document.
//!
//! The banner below is verbatim from the Debian build of nginx 1.30.4, because the two
//! things worth getting right about it are both in the real one: the version arrives on
//! stderr, and the configure line holds two arguments whose values contain spaces.

use std::path::Path;

mod support;

use rastro::collectors::nginx::{Binary, NginxCollector, nginx_binary};
use rastro_collector::{Collector, Presence};

const BANNER: &str = "\
nginx version: nginx/1.30.4
built by gcc 14.2.0 (Debian 14.2.0-19) 
built with OpenSSL 3.5.6 7 Apr 2026
TLS SNI support enabled
configure arguments: --prefix=/etc/nginx --conf-path=/etc/nginx/nginx.conf --user=nginx \
--with-http_ssl_module --with-cc-opt='-g -O2 -fstack-protector-strong' \
--with-ld-opt='-Wl,-z,relro -Wl,-z,now'
";

/// A build that says the least it can: no compiler line, no TLS line, no configure line.
const BARE_BANNER: &str = "nginx version: nginx/1.24.0\n";

fn binary(banner: &str) -> Binary {
    nginx_binary::parse(banner, Path::new("/usr/sbin/nginx")).expect("this banner is nginx's own")
}

fn arguments_of(binary: &Binary) -> Vec<&str> {
    binary
        .configure_arguments
        .iter()
        .map(|argument| argument.as_str())
        .collect()
}

#[test]
fn a_banner_names_the_product_and_its_version() {
    // Act
    let binary = binary(BANNER);

    // Assert
    assert_eq!(binary.product.as_str(), "nginx");
    assert_eq!(binary.version.as_str(), "1.30.4");
    assert_eq!(binary.path.as_str(), "/usr/sbin/nginx");
}

#[test]
fn a_banner_carries_what_the_binary_was_built_with() {
    // Arrange: the TLS library is the one line here that a security patch moves without
    // touching a configuration file.
    // Act
    let binary = binary(BANNER);

    // Assert
    assert_eq!(
        binary.tls_library.expect("this build reports one").as_str(),
        "OpenSSL 3.5.6 7 Apr 2026"
    );
    assert_eq!(
        binary.compiler.expect("this build reports one").as_str(),
        "gcc 14.2.0 (Debian 14.2.0-19)"
    );
}

#[test]
fn a_configure_argument_holding_spaces_stays_one_argument() {
    // Arrange: nginx quotes the two that carry a whole compiler command line. Splitting on
    // every space would report `-O2` as something the binary was configured with.
    // Act
    let binary = binary(BANNER);

    // Assert
    assert_eq!(
        arguments_of(&binary),
        [
            "--prefix=/etc/nginx",
            "--conf-path=/etc/nginx/nginx.conf",
            "--user=nginx",
            "--with-http_ssl_module",
            "--with-cc-opt=-g -O2 -fstack-protector-strong",
            "--with-ld-opt=-Wl,-z,relro -Wl,-z,now",
        ]
    );
}

#[test]
fn a_binary_reads_its_configuration_where_it_was_built_to() {
    // Act
    let binary = binary(BANNER);

    // Assert
    assert_eq!(binary.prefix(), "/etc/nginx");
    assert_eq!(binary.configuration_path(), "/etc/nginx/nginx.conf");
}

#[test]
fn a_build_that_says_nothing_falls_back_to_nginx_s_own_defaults() {
    // Arrange: every distribution's build reports both paths, and a build from source with
    // no switches reports neither. The fallback is what nginx itself would use.
    // Act
    let binary = binary(BARE_BANNER);

    // Assert
    assert_eq!(binary.prefix(), "/usr/local/nginx");
    assert_eq!(binary.configuration_path(), "conf/nginx.conf");
    assert_eq!(binary.tls_library, None);
    assert_eq!(arguments_of(&binary), Vec::<&str>::new());
}

#[test]
fn text_that_is_not_a_banner_is_refused() {
    // Act
    let refused = nginx_binary::parse(
        "bash: nginx: command not found\n",
        Path::new("/usr/sbin/nginx"),
    )
    .expect_err("this is not a banner");

    // Assert
    assert!(refused.to_string().contains("nginx version: "), "{refused}");
}

#[test]
fn a_host_with_no_nginx_is_absent_rather_than_failed() {
    // Arrange
    let collector = NginxCollector::reading(None);

    // Act & Assert: absence is state. A box without nginx serves nothing with it, which is
    // a different fact from rastro being unable to look.
    assert_eq!(collector.presence(), Presence::Absent);
    assert!(collector.collect().is_err());
}

#[test]
fn the_facet_is_named_nginx() {
    // Act & Assert
    assert_eq!(NginxCollector::reading(None).name().as_str(), "nginx");
}

/// A build with the five temp paths a distribution's nginx carries.
const TEMP_PATHS: &str = "\
nginx version: nginx/1.30.4
configure arguments: --prefix=/etc/nginx \
--http-client-body-temp-path=/var/cache/nginx/client_temp \
--http-proxy-temp-path=/var/cache/nginx/proxy_temp \
--http-fastcgi-temp-path=/var/cache/nginx/fastcgi_temp \
--with-http_ssl_module
";

#[test]
fn a_binary_names_the_trees_it_was_built_to_write_into() {
    // Arrange: these are the trees nginx writes into when no directive overrides them, so
    // they are claimed by the walk whether or not the configuration mentions them.
    // Act
    let trees = binary(TEMP_PATHS).working_trees();

    // Assert: the three this build names, and the two it does not — nginx writes into all
    // five, and the two it was never told about sit at its own defaults under the prefix.
    assert_eq!(
        trees,
        [
            "/etc/nginx/scgi_temp",
            "/etc/nginx/uwsgi_temp",
            "/var/cache/nginx/client_temp",
            "/var/cache/nginx/fastcgi_temp",
            "/var/cache/nginx/proxy_temp",
        ]
    );
}

#[test]
fn a_build_that_names_no_temp_path_still_claims_the_five_nginx_uses() {
    // Arrange: a build from source with no switches. It writes into all five trees and says
    // so nowhere, so a facet that only read the configure line would leave them unclaimed
    // and the walk would hash them.
    // Act
    let trees = binary(BARE_BANNER).working_trees();

    // Assert: nginx's own defaults, under nginx's own default prefix.
    assert_eq!(
        trees,
        [
            "/usr/local/nginx/client_body_temp",
            "/usr/local/nginx/fastcgi_temp",
            "/usr/local/nginx/proxy_temp",
            "/usr/local/nginx/scgi_temp",
            "/usr/local/nginx/uwsgi_temp",
        ]
    );
}

#[test]
fn a_relative_temp_path_is_claimed_where_nginx_would_put_it() {
    // Arrange: measured shape — a configure argument may be relative, and a relative tree
    // is one `WalkedTree` refuses, so it would have been dropped from the claims without a
    // word and the walk would have hashed a cache.
    const RELATIVE: &str = "\
nginx version: nginx/1.30.4
configure arguments: --prefix=/srv/nginx --http-proxy-temp-path=spool/proxy
";

    // Act
    let trees = binary(RELATIVE).working_trees();

    // Assert
    assert!(
        trees.contains(&"/srv/nginx/spool/proxy".to_owned()),
        "{trees:?}"
    );
}

#[test]
fn a_host_without_nginx_claims_no_trees() {
    // Arrange: claims are gathered before any collector runs, so this is asked of a box that
    // may have no nginx at all. It must answer nothing rather than fail the run.
    let collector = NginxCollector::reading(None);

    // Act & Assert
    assert!(collector.filesystem_claims().is_empty());
}

#[test]
fn a_banner_that_names_no_version_is_refused() {
    // Arrange: every nginx and every fork spells itself `name/version`. Something without
    // the slash is not a banner rastro can read, and guessing at it would put a version in
    // the document that the box does not have.
    let refused = nginx_binary::parse("nginx version: nginx\n", Path::new("/usr/sbin/nginx"))
        .expect_err("this banner carries no version");

    // Assert
    assert!(refused.to_string().contains('/'), "{refused}");
}
