//! `/etc/security/pam_env.conf`, `pam_env`'s own configuration.
//!
//! One rule per line: a variable name, then `DEFAULT=` and `OVERRIDE=` in either order and
//! either optional. Measured against `libpam-modules` 1.7.0 — `OVERRIDE` wins where both are
//! present, and a value may hold `@{HOME}` or `${USER}`, which is expanded per session at
//! login rather than being what the file says.

use rastro_collector::EnvironmentVariableName;

use crate::collectors::pam::model::EnvironmentRule;

/// The rules a `pam_env.conf` text declares.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EnvironmentRules {
    pub rules: Vec<EnvironmentRule>,
    pub ignored_lines: usize,
}

const DEFAULT: &str = "DEFAULT=";
const OVERRIDE: &str = "OVERRIDE=";

/// Reads the file's text into the rules it declares.
pub fn parse(text: &str) -> EnvironmentRules {
    let mut parsed = EnvironmentRules::default();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        match rule(line) {
            Some(rule) => parsed.rules.push(rule),
            None => parsed.ignored_lines += 1,
        }
    }

    parsed
}

/// One `NAME [DEFAULT=…] [OVERRIDE=…]` line.
///
/// A line naming a variable and nothing else is kept: `pam_env` reads it as a rule that sets
/// nothing, and recording it is how a reader sees that somebody wrote one. A line whose first
/// word is not a legal name is counted instead, since it declares no variable at all.
fn rule(line: &str) -> Option<EnvironmentRule> {
    let mut words = line.split_whitespace();
    let name = EnvironmentVariableName::new(words.next()?).ok()?;

    let mut default = None;
    let mut override_value = None;

    for word in words {
        if let Some(value) = word.strip_prefix(DEFAULT) {
            default = Some(unquoted(value));
        } else if let Some(value) = word.strip_prefix(OVERRIDE) {
            override_value = Some(unquoted(value));
        }
    }

    Some(EnvironmentRule {
        name,
        default,
        override_value,
    })
}

/// A value's surrounding quotes taken off, if it has a matching pair.
fn unquoted(value: &str) -> String {
    for quote in ['"', '\''] {
        if let Some(inside) = value
            .strip_prefix(quote)
            .and_then(|v| v.strip_suffix(quote))
        {
            return inside.to_owned();
        }
    }

    value.to_owned()
}
