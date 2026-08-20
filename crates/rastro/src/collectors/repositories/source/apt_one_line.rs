//! apt's original one-line entry format.
//!
//! `deb [signed-by=/usr/share/keyrings/x.asc] https://apt.example.org/repo suite main`
//!
//! Everything peculiar to this format lives here: a type word, an optional bracketed
//! option list that may contain spaces, then a URI, a suite and any number of
//! components.

use std::collections::BTreeMap;

use rastro_collector::CollectionError;

use crate::collectors::repositories::model::Repository;
use crate::collectors::repositories::value_objects::{
    ArchiveType, Component, Components, Enablement, RepositoryUri, Suite,
};

/// The fewest words an entry can have: a type, a URI and a suite.
const MINIMUM_WORDS: usize = 3;

const OPTIONS_OPEN: char = '[';
const OPTIONS_CLOSE: char = ']';

/// Reads one line, or decides it is not an entry at all.
///
/// **`Ok(None)` means "this line is not a repository", and telling that apart from a
/// disabled one is the whole difficulty of this format.** Both start with `#`. A
/// commented-out entry is state that must be recorded, because swapping a repository
/// out is done by commenting the old line and adding a new one, and dropping the old
/// one would report that as an addition rather than a replacement. But `sources.list`
/// on an ordinary Debian 12 box contains nothing *but* prose comments, and recording
/// `See /etc/apt/sources.list.d/debian.sources` as a disabled repository would be
/// nonsense.
///
/// So the `#` is stripped and the rest is offered to the same parser an enabled line
/// gets. If it parses, it was an entry and is recorded as disabled. If it does not, it
/// was prose and is skipped. The parser decides, rather than a heuristic about what
/// comments look like.
pub fn parse_line(line: &str) -> Result<Option<Repository>, CollectionError> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    match trimmed.strip_prefix('#') {
        // Prose rather than an entry, which is why a failure here is not propagated.
        Some(commented) => Ok(parse_entry(commented.trim(), Enablement::Disabled).ok()),
        None => parse_entry(trimmed, Enablement::Enabled).map(Some),
    }
}

fn parse_entry(entry: &str, enablement: Enablement) -> Result<Repository, CollectionError> {
    let (archive_type, rest) = entry
        .split_once(char::is_whitespace)
        .ok_or_else(|| CollectionError::new(format!("{entry:?} is not an apt repository entry")))?;
    let archive_type = ArchiveType::parse(archive_type)?;

    let (settings, rest) = split_options(rest.trim())?;

    let words: Vec<&str> = rest.split_whitespace().collect();
    let [uri, suite, components @ ..] = words.as_slice() else {
        return Err(CollectionError::new(format!(
            "an apt repository entry needs at least {MINIMUM_WORDS} words, got {:?}",
            entry
        )));
    };

    Ok(Repository {
        uri: RepositoryUri::new(*uri)?,
        enablement,
        archive_type: Some(archive_type),
        suite: Some(Suite::new(*suite)?),
        components: parse_components(components)?,
        tag: None,
        settings,
    })
}

/// Peels the bracketed option list off the front, if there is one.
///
/// **Split on the bracket rather than on whitespace**, because the list may contain
/// spaces: `[arch=amd64 signed-by=/etc/keys/x.asc]` is one option list of two options,
/// and a whitespace split would read `signed-by=...]` as the URI. apt also accepts
/// `[ arch=amd64 ]` with the brackets spaced away from the options, which is why each
/// piece is trimmed.
fn split_options(rest: &str) -> Result<(BTreeMap<String, String>, &str), CollectionError> {
    if !rest.starts_with(OPTIONS_OPEN) {
        return Ok((BTreeMap::new(), rest));
    }

    let close = rest.find(OPTIONS_CLOSE).ok_or_else(|| {
        CollectionError::new(format!(
            "an apt repository entry opens an option list and never closes it: {rest:?}"
        ))
    })?;

    let options = &rest[OPTIONS_OPEN.len_utf8()..close];
    let remainder = &rest[close + OPTIONS_CLOSE.len_utf8()..];

    Ok((parse_options(options), remainder.trim()))
}

/// Reads the options as they are written, without judging which ones apt knows.
///
/// An option with no `=` is kept with an empty value rather than refused. apt warns
/// and ignores such a token, so it is not a repository-defining fact, but it is
/// something an operator typed into the file and dropping it would make the facet
/// quietly incomplete.
fn parse_options(options: &str) -> BTreeMap<String, String> {
    options
        .split_whitespace()
        .map(|option| match option.split_once('=') {
            Some((name, value)) => (name.to_owned(), value.to_owned()),
            None => (option.to_owned(), String::new()),
        })
        .collect()
}

fn parse_components(words: &[&str]) -> Result<Components, CollectionError> {
    let components = words
        .iter()
        .map(|component| Component::new(*component))
        .collect::<Result<Vec<Component>, CollectionError>>()?;

    Ok(Components::new(components))
}
