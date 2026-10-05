#![allow(dead_code)]

//! A TLS listener with a certificate nobody vouches for, as a node's auto-configured one is to
//! anything but the node's own CA.
//!
//! `fixtures/tls/node.crt` is self-signed, names `elasticsearch.invalid`, and was made once with
//! `openssl ecparam -name prime256v1 -genkey -param_enc named_curve`, as PKCS#8, and `openssl req -x509`. Neither the name nor the
//! issuer can satisfy a verifier that checks them, which is the point. `fixtures/tls/other.key` is
//! a second key made the same way, which the certificate does not name.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::Arc;

use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::{ServerConfig, ServerConnection, StreamOwned};

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/tls")
        .join(name)
}

fn certificates() -> Vec<CertificateDer<'static>> {
    CertificateDer::pem_file_iter(fixture("node.crt"))
        .expect("the fixture certificate")
        .collect::<Result<_, _>>()
        .expect("a certificate")
}

/// A listener that demands a client certificate signed by the fixture's own, as a node with
/// `xpack.security.http.ssl.client_authentication: required` does.
pub fn server_config_requiring_a_client_certificate() -> Arc<ServerConfig> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut roots = rustls::RootCertStore::empty();
    for certificate in certificates() {
        roots.add(certificate).expect("a usable root");
    }
    let verifier = rustls::server::WebPkiClientVerifier::builder_with_provider(
        Arc::new(roots),
        Arc::clone(&provider),
    )
    .build()
    .expect("a client verifier");
    let key = PrivateKeyDer::from_pem_file(fixture("node.key")).expect("the fixture key");
    let config = ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .expect("TLS versions ring supports")
        .with_client_cert_verifier(verifier)
        .with_single_cert(certificates(), key)
        .expect("a usable certificate");
    Arc::new(config)
}

pub fn server_config() -> Arc<ServerConfig> {
    let certificates: Vec<CertificateDer<'static>> =
        CertificateDer::pem_file_iter(fixture("node.crt"))
            .expect("the fixture certificate")
            .collect::<Result<_, _>>()
            .expect("a certificate");
    let key = PrivateKeyDer::from_pem_file(fixture("node.key")).expect("the fixture key");
    let config =
        ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .expect("TLS versions ring supports")
            .with_no_client_auth()
            .with_single_cert(certificates, key)
            .expect("a usable certificate");
    Arc::new(config)
}

/// Completes the handshake on `stream`, reads one request's head, and returns it with the
/// stream to answer on.
pub fn accept(stream: TcpStream) -> Option<(String, StreamOwned<ServerConnection, TcpStream>)> {
    let connection = ServerConnection::new(server_config()).ok()?;
    let mut tls = StreamOwned::new(connection, stream);
    let mut request = Vec::new();
    let mut byte = [0_u8; 1];
    while !request.ends_with(b"\r\n\r\n") && tls.read(&mut byte).ok()? == 1 {
        request.push(byte[0]);
    }
    Some((String::from_utf8_lossy(&request).into_owned(), tls))
}

/// Writes `response` and closes as a node does, with a `close_notify`.
pub fn answer(mut tls: StreamOwned<ServerConnection, TcpStream>, response: &[u8]) {
    let _ = tls.write_all(response);
    tls.conn.send_close_notify();
    let _ = tls.flush();
}

/// A listener that serves every connection over TLS with what `respond` makes of its request.
pub fn serving(respond: impl Fn(&str) -> Vec<u8> + Send + 'static) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("a bound port").port();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            if let Some((request, tls)) = accept(stream) {
                let response = respond(&request);
                answer(tls, &response);
            }
        }
    });
    port
}

/// A listener that demands a client certificate and answers nobody without one.
pub fn requiring_a_client_certificate() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("a bound port").port();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let Ok(connection) =
                ServerConnection::new(server_config_requiring_a_client_certificate())
            else {
                continue;
            };
            let mut tls = StreamOwned::new(connection, stream);
            let mut byte = [0_u8; 1];
            let _ = tls.read(&mut byte);
        }
    });
    port
}

/// Presents the fixture's certificate and signs with a key it does not name, as a peer that
/// copied a node's certificate and does not hold its key would.
#[derive(Debug)]
struct NotTheCertificatesKey(Arc<rustls::sign::CertifiedKey>);

impl rustls::server::ResolvesServerCert for NotTheCertificatesKey {
    fn resolve(
        &self,
        _hello: rustls::server::ClientHello<'_>,
    ) -> Option<Arc<rustls::sign::CertifiedKey>> {
        Some(Arc::clone(&self.0))
    }
}

/// A listener presenting the node's certificate without holding its key.
pub fn presenting_a_certificate_it_holds_no_key_for() -> u16 {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let other = PrivateKeyDer::from_pem_file(fixture("other.key")).expect("the other key");
    let signing = provider
        .key_provider
        .load_private_key(other)
        .expect("a usable key");
    let key = rustls::sign::CertifiedKey::new(certificates(), signing);
    let config = ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .expect("TLS versions ring supports")
        .with_no_client_auth()
        .with_cert_resolver(Arc::new(NotTheCertificatesKey(Arc::new(key))));
    let config = Arc::new(config);

    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("a bound port").port();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let Ok(connection) = ServerConnection::new(Arc::clone(&config)) else {
                continue;
            };
            let mut tls = StreamOwned::new(connection, stream);
            let mut byte = [0_u8; 1];
            let _ = tls.read(&mut byte);
        }
    });
    port
}
