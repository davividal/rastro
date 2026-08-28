//! One line `pg_hba_file_rules` found in the client-authentication configuration.

use rastro_collector::Observation;

/// A host-based authentication rule, as the server parsed it from the files.
///
/// Who may connect as whom, from where, and how they must authenticate: server state of the
/// first order that `pg_settings` does not carry at all, since it holds only `hba_file`'s
/// path and only for a privileged role. Every field is text the way the server printed it,
/// because a rule is compared run to run rather than computed with, and the `database`,
/// `user_name` and `options` columns are arrays the server renders as `{...}`.
///
/// **`rule_number` and `file_name` are PostgreSQL 16 additions**, so they are absent on a
/// PostgreSQL 15 cluster, where the view is nine columns rather than eleven. A field is also
/// absent where the server left it null: `address` and `netmask` on a `local` rule, and most
/// columns on a malformed line, which instead carries an `error`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct HbaRule {
    pub rule_number: Option<i64>,
    pub file_name: Option<String>,
    pub line_number: Option<i64>,
    pub connection_type: Option<String>,
    pub databases: Option<String>,
    pub users: Option<String>,
    pub address: Option<String>,
    pub netmask: Option<String>,
    pub auth_method: Option<String>,
    pub options: Option<String>,
    pub error: Option<String>,
}

impl From<&HbaRule> for Observation {
    fn from(rule: &HbaRule) -> Self {
        Observation::object([
            ("rule_number", integer_or_null(rule.rule_number)),
            ("file_name", text_or_null(&rule.file_name)),
            ("line_number", integer_or_null(rule.line_number)),
            ("type", text_or_null(&rule.connection_type)),
            ("database", text_or_null(&rule.databases)),
            ("user_name", text_or_null(&rule.users)),
            ("address", text_or_null(&rule.address)),
            ("netmask", text_or_null(&rule.netmask)),
            ("auth_method", text_or_null(&rule.auth_method)),
            ("options", text_or_null(&rule.options)),
            ("error", text_or_null(&rule.error)),
        ])
    }
}

fn text_or_null(value: &Option<String>) -> Observation {
    match value {
        Some(text) => Observation::text(text.as_str()),
        None => Observation::null(),
    }
}

fn integer_or_null(value: Option<i64>) -> Observation {
    match value {
        Some(number) => Observation::integer(number),
        None => Observation::null(),
    }
}
