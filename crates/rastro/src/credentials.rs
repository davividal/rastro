//! The credentials an operator hands a run, for the collectors that cannot read without one.
//!
//! **Neither the config nor the argv.** The config file is echoed into the document by the
//! `invocation` facet, and the argv is in `/proc/<pid>/cmdline` for any account to read. So a
//! run is given its credentials as a file, or on stdin from a secret manager, one `NAME=value`
//! per line, the shape a `.env` file and every manager's templating already write. What the
//! document records is which names were given, never a value.

use std::collections::BTreeMap;
use std::fmt;
use std::io::Read;
use std::path::Path;

/// The names and values one run was given.
#[derive(Clone, PartialEq, Eq, Default)]
pub struct Credentials {
    values: BTreeMap<String, String>,
}

impl Credentials {
    /// One `NAME=value` per line; blank lines and `#` comments skipped, an `export ` before the
    /// name and quotes around the value taken off, as a shell reading the same file would.
    ///
    /// A refusal names the line by its number and never quotes it: a line that is not a
    /// credential may be the secret itself, pasted without its name.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut values = BTreeMap::new();

        for (index, line) in text.lines().enumerate() {
            let number = index + 1;
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let line = line.strip_prefix("export ").unwrap_or(line);
            let (name, value) = line
                .split_once('=')
                .filter(|(name, _)| is_a_name(name))
                .ok_or_else(|| format!("line {number} of the credentials is not NAME=value"))?;

            if values
                .insert(name.to_owned(), unquoted(value).to_owned())
                .is_some()
            {
                return Err(format!(
                    "line {number} of the credentials gives {name} a second time"
                ));
            }
        }

        Ok(Self { values })
    }

    /// The credentials in the file at `path`, or on stdin where it is `-`.
    pub fn read(path: &Path) -> Result<Self, String> {
        let text = match path == Path::new("-") {
            true => {
                let mut text = String::new();
                std::io::stdin()
                    .read_to_string(&mut text)
                    .map_err(|error| {
                        format!("the credentials on stdin could not be read: {error}")
                    })?;
                text
            }
            false => std::fs::read_to_string(path).map_err(|error| {
                format!(
                    "the credentials in {} could not be read: {error}",
                    path.display()
                )
            })?,
        };

        Self::parse(&text)
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.values.get(name).map(String::as_str)
    }

    /// The names given, in order, which is all of them the document records.
    pub fn names(&self) -> Vec<String> {
        self.values.keys().cloned().collect()
    }
}

/// Names only: a `Debug` that showed values would put them in any panic or log that prints one.
impl fmt::Debug for Credentials {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Credentials")
            .field("names", &self.names())
            .finish()
    }
}

/// Whether a file of credentials at `path` can be read by accounts other than its owner.
///
/// Warned about rather than refused: the secret has already been readable by them, and refusing
/// the run now would not take that back.
pub fn readable_by_other_accounts(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    std::fs::metadata(path).is_ok_and(|metadata| metadata.permissions().mode() & 0o077 != 0)
}

fn is_a_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
}

fn unquoted(value: &str) -> &str {
    ['"', '\'']
        .iter()
        .find_map(|quote| value.strip_prefix(*quote)?.strip_suffix(*quote))
        .unwrap_or(value)
}
