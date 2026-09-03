//! One file of a configuration, and what reading it produced.

use rastro_collector::{AbsolutePath, NonEmptyText, Observation, Xxh3Digest};

use crate::collectors::nginx::model::Directive;
use crate::collectors::nginx::value_objects::FileReading;

/// A file nginx would read, keyed by the path nginx would open.
///
/// The path is the one the configuration names, symlink and all. `sites-enabled/default`
/// pointing at `sites-available/default` is how a vhost is switched on, and recording the
/// target instead would hide the enablement that a fingerprint exists to catch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigurationFile {
    pub path: AbsolutePath,
    pub reading: FileReading,
}

impl ConfigurationFile {
    /// A file that was read, digested by what it says rather than how it is written.
    ///
    /// **The digest is taken over the parsed directives, not the bytes.** A comment added, a
    /// block re-indented or an argument requoted leaves nginx serving exactly what it served
    /// before, and a digest of the bytes would report all three as a change to the service.
    /// What the digest does catch is every directive this facet's model does not name, which
    /// is most of them: a `client_max_body_size` that moved shows here even though no
    /// modelled value moved with it.
    pub fn parsed(path: AbsolutePath, directives: &[Directive]) -> Self {
        Self {
            path,
            reading: FileReading::Parsed {
                digest: Xxh3Digest::of(canonical(directives).as_bytes()),
            },
        }
    }

    pub fn refused(path: AbsolutePath, reason: NonEmptyText) -> Self {
        Self {
            path,
            reading: FileReading::Refused { reason },
        }
    }
}

/// The directives as one unambiguous string, so a digest of it means what it appears to.
///
/// Every token is written with its length in front of it, because a token may hold any
/// character at all — nginx's own escapes make a newline or a tab a legal part of one — and
/// any separator chosen instead could be forged by a value containing it.
fn canonical(directives: &[Directive]) -> String {
    let mut form = String::new();
    append(directives, &mut form);
    form
}

fn append(directives: &[Directive], form: &mut String) {
    for directive in directives {
        append_token(directive.name.as_str(), form);
        for argument in &directive.arguments {
            append_token(argument.as_str(), form);
        }

        match &directive.block {
            Some(block) => {
                form.push('{');
                append(block, form);
                form.push('}');
            }
            None => form.push(';'),
        }
    }
}

fn append_token(token: &str, form: &mut String) {
    form.push_str(&token.len().to_string());
    form.push(':');
    form.push_str(token);
}

impl From<&ConfigurationFile> for Observation {
    fn from(file: &ConfigurationFile) -> Self {
        Observation::object([
            ("path", Observation::text(file.path.as_str())),
            ("reading", Observation::from(&file.reading)),
        ])
    }
}
