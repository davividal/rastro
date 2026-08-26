//! An agent's argument vector, split into the flags it carries.

use std::collections::BTreeMap;

use rastro_collector::CollectionError;

use crate::collectors::exporters::value_objects::{SettingName, SettingValue};

/// The flags an agent was started with, keyed by name.
///
/// # Why this splits an argument vector the units facet deliberately does not
///
/// systemd does not preserve quoting in what it shows, so `--flag="a b"` comes back
/// indistinguishable from two arguments. The units facet therefore keeps the whole vector
/// as one string rather than inventing a structure its source cannot support.
///
/// Here the split is safe for the opposite reason: **a bad split is refutable.** Every one
/// of these agents takes `--flag=value`, so a token that is not a flag can only mean the
/// vector was not what rastro assumed, and that is a recorded failure naming the argument.
/// The units facet has no such rule available, because a unit may start anything at all.
///
/// The first token is dropped: it is `argv[0]`, which is the binary and not a setting.
pub fn parse(argv: &str) -> Result<BTreeMap<SettingName, Option<SettingValue>>, CollectionError> {
    let mut settings = BTreeMap::new();

    for argument in argv.split_whitespace().skip(1) {
        let (name, value) = flag(argument)?;

        if settings.insert(name.clone(), value).is_some() {
            return Err(CollectionError::new(format!(
                "the agent is passed {:?} more than once, so the argument vector was misread",
                name.as_str()
            )));
        }
    }

    Ok(settings)
}

/// One argument, split into the flag it names and the value it carries.
///
/// **Only the `=` form is understood, and anything else is refused rather than guessed.**
/// Go's flag package also accepts `--flag value`, which is indistinguishable from a boolean
/// flag followed by a positional argument unless you know every flag's type. These agents
/// have hundreds between them and the sets move between releases, so rastro would be
/// maintaining a copy of somebody else's flag table and reporting a wrong configuration
/// whenever the copy fell behind.
fn flag(argument: &str) -> Result<(SettingName, Option<SettingValue>), CollectionError> {
    // Both spellings, because Go treats them as the same flag; the name loses them.
    let flag = argument
        .strip_prefix("--")
        .or_else(|| argument.strip_prefix('-'))
        .ok_or_else(|| {
            CollectionError::new(format!(
                "the agent is passed {argument:?}, which is not a flag, so rastro cannot say \
                 which setting it belongs to"
            ))
        })?;

    match flag.split_once('=') {
        Some((name, value)) => Ok((SettingName::new(name)?, Some(SettingValue::new(value)?))),
        None => Ok((SettingName::new(flag)?, None)),
    }
}
