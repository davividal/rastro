//! Reading the certificate a virtual host serves, and describing the key beside it.
//!
//! The certificate below is a real self-signed one, generated for this test with a hundred
//! years of validity so that nothing here is a clock-dependent assertion. It carries two DNS
//! names and an IP address, because the three appear together on a real server certificate
//! and the extension spells the address as bytes rather than as text.

use std::path::PathBuf;

mod support;

use rastro::collectors::nginx::model::{CertificateReading, KeyReading};
use rastro::collectors::nginx::source::ConfigurationFiles;
use rastro::collectors::nginx::value_objects::ConfigurationSource;
use rastro::collectors::nginx::{VirtualHost, nginx_directives};
use support::fs_tree::{scratch_tree, write};

const CERTIFICATE: &str = "\
-----BEGIN CERTIFICATE-----
MIIDKDCCAhCgAwIBAgIJAJA1jBfa7rJoMA0GCSqGSIb3DQEBCwUAMDkxCzAJBgNV
BAYTAkRFMRQwEgYDVQQKDAtyYXN0cm8gdGVzdDEUMBIGA1UEAwwLZXhhbXBsZS5v
cmcwIBcNMjYwOTAzMDYzNzA1WhgPMjEyNjA4MTAwNjM3MDVaMDkxCzAJBgNVBAYT
AkRFMRQwEgYDVQQKDAtyYXN0cm8gdGVzdDEUMBIGA1UEAwwLZXhhbXBsZS5vcmcw
ggEiMA0GCSqGSIb3DQEBAQUAA4IBDwAwggEKAoIBAQDTbeJVGpKMLx36SOuWMXdZ
bOWp/Oq9diGb7YKqnQw6NiRu7HfCDbku8uwsKHpPcUds2cXjq0XlKlg+pHlfcRtm
JKrNYXHt4B5eKrr8JKi+p1fCu3ljv2UvkJJ1URANbTMSeJsbv584B1GRphhThUGm
nlbJG0QQIAWt1zoqwFYH3p+XqdgtqBaQ7f8yaPuD2PP4zAbpVtjP2DIYsb2V4Dbz
9G6wwMhXjm1v3dpE1NKbClWLV10yvDBsVfDGlFgVJgohtOjrvSrQsQ92Tl13X5e1
PZElirk6ryUdMhj3Uw1ZV7GcSVhSQFdKU1+umSgG+EiQwGB/AGkHsjRjdR8ZN2rP
AgMBAAGjMTAvMC0GA1UdEQQmMCSCC2V4YW1wbGUub3Jngg93d3cuZXhhbXBsZS5v
cmeHBAoAAAcwDQYJKoZIhvcNAQELBQADggEBAAPx18AJ/QX3NwiaEkm9A5iEgRXp
qf5XLPInemAwtUPR9lWNkMtPdyeoE90u/1YtwYLKNxvd00QKAdc14Wo9KXzkvY5Q
1DC8V05aXV4z7DrSnEsxSwEglfkMbUEPrCJIY8jfGnnANFUIQeC3+8l3OxSi8H1l
h4DnJs2OgUc/xohKM2az4KbVsw6LvVjVYCZ+ykrvcTcLIzBUPaQswOkGTNzIT9bd
wiQgYL9W7xGgM40TOT5E4RtIDWLq4pYW9m1NgchVc6b9vaxRVkUE21vjEKNSFzI+
u36fKdDNmzvfeZ3+B+d9V/QSBZboaFmkzsUStyiI+6REt6MTu+j3R4TAe90=
-----END CERTIFICATE-----
";

/// A stand-in for a private key. Nothing opens it, which is the point of the test below.
const KEY: &str = "-----BEGIN PRIVATE KEY-----\nnot a key\n-----END PRIVATE KEY-----\n";

fn host(configuration: &str, name: &str) -> VirtualHost {
    let prefix = fixture(configuration, name);
    let read = ConfigurationFiles::at(
        prefix.join("nginx.conf"),
        &prefix,
        ConfigurationSource::CompiledIn,
    )
    .expect("the scratch tree is absolute")
    .read();

    nginx_directives::http_service(&read.directives, &prefix)
        .expect("this configuration holds no directive rastro cannot read")
        .hosts
        .remove(0)
}

fn fixture(configuration: &str, name: &str) -> PathBuf {
    let prefix = scratch_tree(&format!("nginx-certificates-{name}"), &[]);
    write(&prefix, "nginx.conf", configuration);
    write(&prefix, "example.org.crt", CERTIFICATE);
    write(&prefix, "example.org.key", KEY);
    prefix
}

#[test]
fn a_certificate_is_read_from_the_file_the_configuration_names() {
    // Arrange: a relative path, which nginx resolves against the prefix.
    let host = host(
        "http { server { ssl_certificate example.org.crt; ssl_certificate_key example.org.key; } }",
        "read",
    );

    // Act
    let CertificateReading::Parsed(details) = &host.certificates[0].reading else {
        panic!("the certificate is a readable file");
    };

    // Assert
    assert!(
        details.subject.as_str().contains("CN=example.org"),
        "{:?}",
        details.subject
    );
    assert_eq!(details.issuer, details.subject);
    // The serial as a number: DER pads a high-bit serial with a leading zero byte, and
    // that padding is not part of it.
    assert_eq!(details.serial.as_str(), "90358c17daeeb268");
    assert_eq!(details.key_algorithm.as_str(), "rsaEncryption");

    // Sorted, because the order in the extension is the issuer's rather than the operator's.
    assert_eq!(
        details
            .subject_alternative_names
            .iter()
            .map(|name| name.as_str())
            .collect::<Vec<&str>>(),
        ["10.0.0.7", "example.org", "www.example.org"]
    );

    // 2026-09-03 to 2126-08-10, as whole seconds. An instant, never a countdown: days
    // remaining would differ between two runs of an unchanged host.
    assert_eq!(details.not_before.as_i64(), 1_788_417_425);
    assert_eq!(details.not_after.as_i64(), 4_942_017_425);
}

#[test]
fn the_key_is_described_and_never_opened() {
    // Arrange
    let host = host(
        "http { server { ssl_certificate example.org.crt; ssl_certificate_key example.org.key; } }",
        "key",
    );

    // Act
    let key = host.certificates[0]
        .key_file
        .as_ref()
        .expect("this host declares a key");

    // Assert: the mode and the owner, which is what catches a key that became readable.
    // Nothing here could carry the key's content, because nothing reads it.
    assert!(key.path.as_str().ends_with("/example.org.key"));
    let KeyReading::Described { mode, .. } = &key.reading else {
        panic!("the key file is there to be described");
    };
    assert_eq!(mode.as_str(), "0640");
}

#[test]
fn a_certificate_that_is_not_there_is_refused_by_name() {
    // Arrange
    let host = host("http { server { ssl_certificate gone.crt; } }", "missing");

    // Act
    let CertificateReading::Refused { reason } = &host.certificates[0].reading else {
        panic!("there is no such file");
    };

    // Assert
    assert!(reason.as_str().contains("could not be read"), "{reason:?}");
    assert_eq!(host.certificates[0].certificate.as_str(), "gone.crt");
}

#[test]
fn a_certificate_named_through_a_variable_is_not_a_missing_file() {
    // Arrange: nginx resolves this per request, so there is no file to read and saying
    // "could not be read" would be wrong about the host.
    let host = host(
        "http { server { ssl_certificate /etc/ssl/$ssl_server_name.crt; } }",
        "variable",
    );

    // Act
    let CertificateReading::Refused { reason } = &host.certificates[0].reading else {
        panic!("a variable names no file");
    };

    // Assert
    assert!(reason.as_str().contains("per request"), "{reason:?}");
}

#[test]
fn a_file_that_is_not_a_certificate_is_refused_rather_than_believed() {
    // Arrange
    let prefix = scratch_tree("nginx-certificates-not-pem", &[]);
    write(
        &prefix,
        "nginx.conf",
        "http { server { ssl_certificate junk.crt; } }",
    );
    write(&prefix, "junk.crt", "this is not a certificate\n");
    let read = ConfigurationFiles::at(
        prefix.join("nginx.conf"),
        &prefix,
        ConfigurationSource::CompiledIn,
    )
    .expect("the scratch tree is absolute")
    .read();

    // Act
    let hosts = nginx_directives::http_service(&read.directives, &prefix)
        .expect("a bad certificate costs the certificate, not the facet")
        .hosts;

    // Assert
    let CertificateReading::Refused { reason } = &hosts[0].certificates[0].reading else {
        panic!("this file is not PEM");
    };
    assert!(reason.as_str().contains("not PEM"), "{reason:?}");
}

/// A certificate carrying every kind of alternative name a web server's does: two forms of
/// address, a hostname, an address for a human, and a URI.
const MANY_NAMES: &str = "\
-----BEGIN CERTIFICATE-----
MIIDGzCCAgOgAwIBAgIJAP180G/YhMyOMA0GCSqGSIb3DQEBCwUAMBsxGTAXBgNV
BAMMEG1hbnkuZXhhbXBsZS5vcmcwIBcNMjYwOTA0MTI1NjE0WhgPMjEyNjA4MTEx
MjU2MTRaMBsxGTAXBgNVBAMMEG1hbnkuZXhhbXBsZS5vcmcwggEiMA0GCSqGSIb3
DQEBAQUAA4IBDwAwggEKAoIBAQCnz7Q9yda67AgY+MhNoK1FuUfQwmsyi1TTtzZC
1o8+KdOS2pMA20n5h9dryNflRKktqDPF92cHS+d0+GusUdSGFJbzTP6Pa6Wud/E6
UlU6i5PH5Szm7Q8lZbAWQ0BuLTFJ6azhsqbA/gj+bN8UNq4CQ4hbzZtLBG2Lseld
574JzBgPfgZn9B6Nfra7uV+RxX1+LVkb3qddYHonMQVlXqn0Sx4hLCCcA5W55XVL
8s2fwIEHEFtY6TUNeFlEq+vFpyspckrOQbjyDnA0md8WtNsS1tfveQDTI05nDHNv
RHd6JVhFLBRnpVSJFjz1w4wVNrZQ4XQt2My+IO33b4Ai122/AgMBAAGjYDBeMFwG
A1UdEQRVMFOCEG1hbnkuZXhhbXBsZS5vcmeHBAoAAAeHECABDbgAAAAAAAAAAAAA
AAGBD29wc0BleGFtcGxlLm9yZ4YWaHR0cHM6Ly9leGFtcGxlLm9yZy9jYTANBgkq
hkiG9w0BAQsFAAOCAQEAXJFgGpjC+IxabvOrgwDVFCBSCU+WT8W2lqgRfJYJyVBD
2VX3afSX1WEClgOpYDh8pLVAyeGmYipUOd4hnaC2QjHgNkdJjC6YD621QhoN6jzI
i1XILxCyNQBpdC7xqcqUqjPtr0MBvdXUoNgwVbWnXPmztdmztHkN/wCuEW/Utl4U
vT5Gfs6V3llrWTxr487CTBoKrB9E8DR8Z1Q4DNIRD5FL0kb9jLtwpuPaZEJXHeuY
6jntKEy0Ax5obDpfdqkAY6SED98wh0REraV6Uv1mrNd1uXz0rnTYe0UrQlqAuAdk
c1XQo6kp0MD/qSy2EIkir9u+323NCEv6cgEpPCMWfg==
-----END CERTIFICATE-----
";

#[test]
fn every_kind_of_alternative_name_reaches_the_document_as_itself() {
    // Arrange: the parser's own rendering is a debugging form — `DNSName(example.org)`,
    // and an address as raw bytes — which would put the extension's vocabulary in the
    // document and make a name unusable as a name.
    let prefix = scratch_tree("nginx-certificates-many-names", &[]);
    write(
        &prefix,
        "nginx.conf",
        "http { server { ssl_certificate many.crt; } }",
    );
    write(&prefix, "many.crt", MANY_NAMES);
    let read = ConfigurationFiles::at(
        prefix.join("nginx.conf"),
        &prefix,
        ConfigurationSource::CompiledIn,
    )
    .expect("the scratch tree is absolute")
    .read();
    let hosts = nginx_directives::http_service(&read.directives, &prefix)
        .expect("this configuration holds no directive rastro cannot read")
        .hosts;

    // Act
    let CertificateReading::Parsed(details) = &hosts[0].certificates[0].reading else {
        panic!("the certificate is a readable file");
    };

    // Assert: four bytes render as IPv4 and sixteen as IPv6, both in std's spelling.
    assert_eq!(
        details
            .subject_alternative_names
            .iter()
            .map(|name| name.as_str())
            .collect::<Vec<&str>>(),
        [
            "10.0.0.7",
            "2001:db8::1",
            "https://example.org/ca",
            "many.example.org",
            "ops@example.org"
        ]
    );
    assert_eq!(details.serial.as_str(), "fd7cd06fd884cc8e");
}

#[test]
fn a_key_that_cannot_be_described_says_so() {
    // Arrange: an unprivileged run cannot stat a key under a directory it may not enter, and
    // a key file recorded with no facts at all would read as one nobody has to worry about.
    let host = host(
        "http { server { ssl_certificate example.org.crt;\n\
         ssl_certificate_key /nowhere-at-all/example.key; } }",
        "unreadable-key",
    );

    // Act
    let key = host.certificates[0]
        .key_file
        .as_ref()
        .expect("the configuration names a key");

    // Assert
    let KeyReading::Refused { reason } = &key.reading else {
        panic!("there is no such key file");
    };
    assert!(
        reason.as_str().contains("could not be described"),
        "{reason:?}"
    );
}
