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
