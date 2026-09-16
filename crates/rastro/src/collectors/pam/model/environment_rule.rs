//! One line of `/etc/security/pam_env.conf`.

use rastro_collector::{EnvironmentVariableName, Observation};

/// A variable `pam_env` sets from its own configuration, and how.
///
/// **`DEFAULT` applies when the variable is not already set; `OVERRIDE` wins whether it is
/// or not.** Measured: a line carrying both reaches the session with the `OVERRIDE` value.
///
/// # The recorded value is the rule, not the result
///
/// A value here may hold `@{HOME}` or `${USER}`, which `pam_env` expands **per session, per
/// account**, at login. Measured: `EXPANDED DEFAULT=@{HOME}/x` reaches one account's session
/// as `/home/probe/x`. So what this records is what the file says and not what any
/// particular login receives, and it cannot be otherwise without rastro performing a login
/// for every account — which would be the very mutation this project refuses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvironmentRule {
    pub name: EnvironmentVariableName,
    pub default: Option<String>,
    pub override_value: Option<String>,
}

impl From<&EnvironmentRule> for Observation {
    fn from(rule: &EnvironmentRule) -> Self {
        Observation::object([
            (
                "default",
                match &rule.default {
                    // Sensitive for the same reason a unit's value is: a name says which
                    // variable a session depends on, a value is where a credential would be.
                    Some(value) => Observation::text(value).sensitive(),
                    None => Observation::null(),
                },
            ),
            ("name", Observation::text(rule.name.as_str())),
            (
                "override",
                match &rule.override_value {
                    Some(value) => Observation::text(value).sensitive(),
                    None => Observation::null(),
                },
            ),
        ])
    }
}
