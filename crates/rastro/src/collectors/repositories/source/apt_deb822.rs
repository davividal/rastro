//! apt's deb822 entry format, the one Debian 12 ships by default.
//!
//! ```text
//! Types: deb deb-src
//! URIs: mirror+file:///etc/apt/mirrors/debian.list
//! Suites: bookworm bookworm-updates bookworm-backports
//! Components: main
//! Signed-By: /usr/share/keyrings/debian-archive-keyring.gpg
//! ```
//!
//! Everything peculiar to this format lives here: paragraphs separated by blank lines,
//! `Field: value` with continuation lines that begin with whitespace, and three fields
//! that are lists whose combinations each describe a separate repository.

use std::collections::BTreeMap;

use rastro_collector::CollectionError;

use crate::collectors::repositories::model::Repository;
use crate::collectors::repositories::value_objects::{
    ArchiveType, Component, Components, Enablement, RepositoryUri, Suite,
};

/// The fields that identify a repository rather than configure one.
///
/// Everything outside this set is carried through to the repository's settings
/// verbatim, which is what keeps the facet complete as apt gains fields.
const TYPES: &str = "types";
const URIS: &str = "uris";
const SUITES: &str = "suites";
const COMPONENTS: &str = "components";
const ENABLED: &str = "enabled";

/// A continuation line standing for a blank line inside a value.
///
/// deb822's own convention, and it matters here because an inline armoured signing key
/// contains blank lines and would otherwise end the paragraph.
const BLANK_CONTINUATION: &str = ".";

/// Translates the file's text into repositories.
///
/// **One paragraph can describe many repositories, and expanding them is deliberate.**
/// The paragraph above lists two types and three suites, which is six repositories,
/// and that is exactly what apt fetches. Recording the paragraph as written would
/// leave the facet in the format's vocabulary rather than rastro's, so a box that
/// expressed the same six repositories in six one-line entries would produce a
/// different-looking fingerprint for an identical configuration. Expanding is what
/// makes the two formats comparable, which is the whole job of this layer.
pub fn parse(text: &str) -> Result<Vec<Repository>, CollectionError> {
    let mut repositories = Vec::new();

    for paragraph in paragraphs(text) {
        repositories.extend(expand(&paragraph)?);
    }

    Ok(repositories)
}

/// Splits the file into paragraphs, folding continuation lines into their field.
///
/// Comments are dropped here rather than recorded. Unlike the one-line format, a
/// commented-out deb822 paragraph is not how anybody disables a repository: the format
/// has `Enabled: no` for that, so a `#` here really is prose.
fn paragraphs(text: &str) -> Vec<BTreeMap<String, String>> {
    let mut found = Vec::new();
    let mut current: BTreeMap<String, String> = BTreeMap::new();
    let mut last_field: Option<String> = None;

    for line in text.lines() {
        if line.trim().is_empty() {
            push_paragraph(&mut found, &mut current);
            last_field = None;
            continue;
        }

        if line.starts_with('#') {
            continue;
        }

        if append_continuation(line, &mut current, last_field.as_deref()) {
            continue;
        }

        last_field = parse_field(line, &mut current);
    }

    push_paragraph(&mut found, &mut current);
    found
}

fn push_paragraph(
    found: &mut Vec<BTreeMap<String, String>>,
    current: &mut BTreeMap<String, String>,
) {
    if !current.is_empty() {
        found.push(std::mem::take(current));
    }
}

fn append_continuation(
    line: &str,
    current: &mut BTreeMap<String, String>,
    last_field: Option<&str>,
) -> bool {
    if !line.starts_with([' ', '\t']) {
        return false;
    }

    if let Some(field) = last_field {
        let continued = line.trim();
        let continued = if continued == BLANK_CONTINUATION {
            ""
        } else {
            continued
        };
        let value = current.entry(field.to_owned()).or_default();
        value.push('\n');
        value.push_str(continued);
    }

    true
}

fn parse_field(line: &str, current: &mut BTreeMap<String, String>) -> Option<String> {
    let (name, value) = line.split_once(':')?;
    let name = name.trim().to_ascii_lowercase();
    current.insert(name.clone(), value.trim().to_owned());
    Some(name)
}

/// Every repository one paragraph describes.
fn expand(paragraph: &BTreeMap<String, String>) -> Result<Vec<Repository>, CollectionError> {
    let types = words_of(paragraph, TYPES);
    let uris = words_of(paragraph, URIS);
    let suites = words_of(paragraph, SUITES);

    // All three are checked, and the emptiness of `types` is the one that matters most:
    // the loops below are a cross product, so an absent `Types` would quietly produce no
    // repositories at all and drop the paragraph from a facet claiming to be complete.
    // apt refuses such a paragraph too.
    if types.is_empty() || uris.is_empty() || suites.is_empty() {
        return Err(CollectionError::new(format!(
            "a deb822 repository paragraph needs {TYPES:?}, {URIS:?} and {SUITES:?}, got \
             fields {:?}",
            paragraph.keys().collect::<Vec<&String>>()
        )));
    }

    let components = parse_components(&words_of(paragraph, COMPONENTS))?;
    let enablement = paragraph
        .get(ENABLED)
        .map_or(Enablement::Enabled, |value| Enablement::parse(value));
    let settings = settings_of(paragraph);

    let mut repositories = Vec::new();
    for archive_type in &types {
        for uri in &uris {
            for suite in &suites {
                repositories.push(Repository {
                    uri: RepositoryUri::new(*uri)?,
                    enablement,
                    archive_type: Some(ArchiveType::parse(archive_type)?),
                    suite: Some(Suite::new(*suite)?),
                    components: components.clone(),
                    tag: None,
                    settings: settings.clone(),
                });
            }
        }
    }

    Ok(repositories)
}

/// The fields that are not part of a repository's identity, kept as written.
///
/// `Signed-By` is among them, and it may be an entire armoured public key folded over
/// continuation lines rather than a keyring path. It is recorded either way: the key is
/// public, and which key signs a repository is exactly the kind of change worth seeing.
/// The cost is that such an entry makes this facet a few kilobytes larger.
fn settings_of(paragraph: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    paragraph
        .iter()
        .filter(|(name, _)| ![TYPES, URIS, SUITES, COMPONENTS, ENABLED].contains(&name.as_str()))
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect()
}

fn words_of<'a>(paragraph: &'a BTreeMap<String, String>, field: &str) -> Vec<&'a str> {
    paragraph
        .get(field)
        .map(|value| value.split_whitespace().collect())
        .unwrap_or_default()
}

fn parse_components(words: &[&str]) -> Result<Components, CollectionError> {
    let components = words
        .iter()
        .map(|component| Component::new(*component))
        .collect::<Result<Vec<Component>, CollectionError>>()?;

    Ok(Components::new(components))
}
