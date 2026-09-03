//! Reading a certificate, and describing the key beside it.
//!
//! **Only the certificate is opened.** It is the public half — every client that connects is
//! handed a copy — so reading it puts nothing in the document that the server does not
//! already give away. The private key is `stat`ed and never opened, which is why this module
//! has two entry points rather than one.
//!
//! PEM, because that is what nginx reads: `ssl_certificate` takes a PEM file that may hold a
//! whole chain, leaf first. Only the leaf is described, since the intermediates are the
//! authority's and change when it says so.

use std::fs;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use rastro_collector::{AbsolutePath, CollectionError, NonEmptyText, Xxh3Digest};
use x509_parser::extensions::GeneralName;
use x509_parser::objects::{oid_registry, oid2sn};
use x509_parser::pem::parse_x509_pem;

use crate::collectors::file_metadata::FileMode;
use crate::collectors::nginx::model::{CertificateDetails, KeyFile, KeyReading};
use crate::collectors::nginx::value_objects::SecondsSinceEpoch;

/// Reads the leaf certificate of a PEM file.
///
/// The serial is recorded as the number it is, in hexadecimal, rather than as the bytes DER
/// stores it in: a serial whose high bit is set is padded with a leading zero byte in the
/// file, so the raw form and the one `openssl x509 -serial` prints differ for half of all
/// certificates. The number is the same either way, and it is the number an operator
/// compares.
pub fn read(path: &Path) -> Result<CertificateDetails, CollectionError> {
    let bytes = fs::read(path).map_err(|error| {
        CollectionError::new(format!(
            "the certificate {} could not be read: {error}",
            path.display()
        ))
    })?;

    let (_, pem) = parse_x509_pem(&bytes).map_err(|error| {
        CollectionError::new(format!(
            "the certificate {} is not PEM: {error}",
            path.display()
        ))
    })?;

    let certificate = pem.parse_x509().map_err(|error| {
        CollectionError::new(format!(
            "the certificate {} is not an X.509 certificate: {error}",
            path.display()
        ))
    })?;

    let mut names = alternative_names(&certificate)?;
    names.sort();

    Ok(CertificateDetails {
        subject: NonEmptyText::new(certificate.subject().to_string(), "certificate subject")?,
        issuer: NonEmptyText::new(certificate.issuer().to_string(), "certificate issuer")?,
        serial: NonEmptyText::new(
            certificate.tbs_certificate.serial.to_str_radix(16),
            "certificate serial",
        )?,
        not_before: SecondsSinceEpoch::new(certificate.validity().not_before.timestamp()),
        not_after: SecondsSinceEpoch::new(certificate.validity().not_after.timestamp()),
        subject_alternative_names: names,
        key_algorithm: key_algorithm(&certificate)?,
        digest: Xxh3Digest::of(&pem.contents),
    })
}

/// Describes the private key without opening it.
pub fn describe_key(path: &AbsolutePath) -> KeyFile {
    let reading = match fs::symlink_metadata(path.as_str()) {
        Ok(metadata) => KeyReading::Described {
            mode: FileMode::of(metadata.mode()),
            owner: metadata.uid().into(),
            group: metadata.gid().into(),
        },
        Err(error) => KeyReading::Refused {
            reason: NonEmptyText::new(
                format!("the key could not be described: {error}"),
                "key file refusal",
            )
            .expect("the message above is not empty"),
        },
    };

    KeyFile {
        path: path.clone(),
        reading,
    }
}

/// Every name the certificate is valid for, in rastro's spelling rather than the parser's.
///
/// The library's own rendering is `DNSName(example.org)`, which is a debugging form: it
/// would put the extension's vocabulary in the document and make a name unusable as a name.
/// The four kinds that appear on a web server's certificate are unwrapped; anything else is
/// kept in the library's form, which at least says what it was.
fn alternative_names(
    certificate: &x509_parser::certificate::X509Certificate<'_>,
) -> Result<Vec<NonEmptyText>, CollectionError> {
    let Ok(Some(extension)) = certificate.subject_alternative_name() else {
        return Ok(Vec::new());
    };

    extension
        .value
        .general_names
        .iter()
        .map(|name| NonEmptyText::new(spelled(name), "subject alternative name"))
        .collect()
}

fn spelled(name: &GeneralName<'_>) -> String {
    match name {
        GeneralName::DNSName(name) => (*name).to_owned(),
        GeneralName::RFC822Name(address) => (*address).to_owned(),
        GeneralName::URI(uri) => (*uri).to_owned(),
        GeneralName::IPAddress(bytes) => address_of(bytes),
        other => other.to_string(),
    }
}

/// An IP address extension holds the address as bytes rather than as text.
fn address_of(bytes: &[u8]) -> String {
    if let Ok(four) = <[u8; 4]>::try_from(bytes) {
        return Ipv4Addr::from(four).to_string();
    }

    match <[u8; 16]>::try_from(bytes) {
        Ok(sixteen) => Ipv6Addr::from(sixteen).to_string(),
        Err(_) => format!("{bytes:02x?}"),
    }
}

/// The algorithm of the public key, by its short name where the registry knows one.
fn key_algorithm(
    certificate: &x509_parser::certificate::X509Certificate<'_>,
) -> Result<NonEmptyText, CollectionError> {
    let oid = &certificate.public_key().algorithm.algorithm;
    let name = oid2sn(oid, oid_registry())
        .map(str::to_owned)
        .unwrap_or_else(|_| oid.to_id_string());

    NonEmptyText::new(name, "certificate key algorithm")
}
