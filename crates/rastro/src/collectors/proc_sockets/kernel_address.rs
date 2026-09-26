//! How the inet tables spell an address: hexadecimal words in host order.
//!
//! Shared, because two readers need the address itself rather than only the port: `sockets`
//! reports what the box offers where, and `redis` has to know which of a server's own
//! addresses it may connect to.

use std::net::{Ipv4Addr, Ipv6Addr};

use rastro_collector::CollectionError;

/// Decodes the four bytes of an IPv4 address.
///
/// **The kernel prints a network-order address reinterpreted as a host-order word**, so
/// `0100007F` is 127.0.0.1 rather than 1.0.0.127. Parsing the hexadecimal back into a
/// `u32` and taking its *native*-endian bytes undoes exactly that reinterpretation, on any
/// architecture: it recovers the bytes the kernel had in memory, which are the address.
pub fn ipv4_of(hexadecimal: &str) -> Result<Ipv4Addr, CollectionError> {
    let word = word_of(hexadecimal, 8)?;

    Ok(Ipv4Addr::from(word.to_ne_bytes()))
}

/// Decodes the sixteen bytes of an IPv6 address.
///
/// Four words, each reinterpreted the same way an IPv4 address is. Reversing the whole
/// sixteen bytes instead produces a plausible and entirely wrong address, which is why this
/// is word by word.
pub fn ipv6_of(hexadecimal: &str) -> Result<Ipv6Addr, CollectionError> {
    if hexadecimal.len() != 32 {
        return Err(CollectionError::new(format!(
            "{hexadecimal:?} is not a 16-byte address, so the row was misread"
        )));
    }

    let mut bytes = [0_u8; 16];
    for (index, word) in hexadecimal.as_bytes().chunks(8).enumerate() {
        let word = word_of(
            std::str::from_utf8(word).expect("a slice of ASCII hexadecimal"),
            8,
        )?;
        bytes[index * 4..(index + 1) * 4].copy_from_slice(&word.to_ne_bytes());
    }

    Ok(Ipv6Addr::from(bytes))
}

fn word_of(hexadecimal: &str, width: usize) -> Result<u32, CollectionError> {
    if hexadecimal.len() != width {
        return Err(CollectionError::new(format!(
            "{hexadecimal:?} is not a {width}-digit address word, so the row was misread"
        )));
    }

    u32::from_str_radix(hexadecimal, 16).map_err(|error| {
        CollectionError::new(format!("{hexadecimal:?} is not an address word: {error}"))
    })
}
