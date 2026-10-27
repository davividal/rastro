//! How `INFO` spells a section: `# Heading` lines, then `name:value` lines, CRLF-terminated.
//!
//! ```text
//! # Server
//! redis_version:7.0.15
//! redis_mode:standalone
//! ```

use std::collections::BTreeMap;

/// The line that starts a section, which carries no field.
const HEADING: char = '#';

/// Every `name:value` line of a reply.
///
/// **Split at the first colon only.** A value can hold colons, `os:Linux 6.1.0 x86_64` does not
/// but a path or an address may, and splitting on every colon would cut them.
pub fn info_fields(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .map(|line| line.trim_end_matches('\r'))
        .filter(|line| !line.is_empty() && !line.starts_with(HEADING))
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.to_owned(), value.to_owned()))
        .collect()
}
