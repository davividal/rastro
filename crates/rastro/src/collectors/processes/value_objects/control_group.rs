//! Which control group a process belongs to.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// A process's control group path.
///
/// `/init.scope`, `/system.slice/ssh.service`, `/user.slice/user-1000.slice/session-4.scope`.
///
/// **The field that ties this facet to the `units` one**, and the reason it is worth
/// collecting at all: on a systemd box the cgroup path names the unit a process belongs to,
/// so a process appearing under `/system.slice/something.service` is how you learn which
/// unit spawned it. Nothing else in the process table says that.
///
/// Text rather than a path type: the value is a cgroup hierarchy path, not a filesystem one,
/// and under cgroup v2 the line `/proc/<pid>/cgroup` carries is `0::` followed by it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ControlGroup(NonEmptyText);

/// The prefix and suffix bounding a login session's scope in a cgroup path.
const SESSION_PREFIX: &str = "session-";
const SESSION_SUFFIX: &str = ".scope";

impl ControlGroup {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "control group")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Whether this path runs through a login session's scope.
    ///
    /// **The same counter the `units` facet already drops, arriving by a different route,
    /// and finding it here was the whole reason this collector needed a second look.** Two
    /// runs over separate ssh connections disagreed on the `rastro`, `sudo`, `sh` and `sshd`
    /// entries — not on their pids, which are dropped, but on this field:
    /// `/user.slice/user-1000.slice/session-851.scope` became `session-852.scope`.
    ///
    /// So rastro observing the box changes what it observes, and the churn is guaranteed
    /// rather than likely: every invocation over ssh creates a new session. The field is
    /// dropped from the diffable view for a process inside one, while the process itself
    /// stays. A path with no session component — `/system.slice/ssh.service`, `/init.scope`
    /// — is untouched, and those are the ones that carry the unit an operator wants.
    pub fn names_a_login_session(&self) -> bool {
        self.as_str().split('/').any(|component| {
            component
                .strip_prefix(SESSION_PREFIX)
                .and_then(|rest| rest.strip_suffix(SESSION_SUFFIX))
                .is_some_and(|counter| {
                    !counter.is_empty() && counter.bytes().all(|byte| byte.is_ascii_digit())
                })
        })
    }
}

impl From<&ControlGroup> for Observation {
    fn from(group: &ControlGroup) -> Self {
        let observed = Observation::text(group.as_str());

        if group.names_a_login_session() {
            return observed.volatile();
        }

        observed
    }
}
