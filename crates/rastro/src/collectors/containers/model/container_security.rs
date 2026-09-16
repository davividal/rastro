//! What a container is allowed to do to the box.

use rastro_collector::{NonEmptyText, Observation};

use crate::collectors::containers::model::{ContainerCapabilities, ContainerNamespaces};

/// The confinement a container runs under.
///
/// One node rather than eight fields scattered through the container, because this is the
/// group somebody reads together: an auditor asking "what can this container do to the host"
/// wants the privileged flag, the capability delta, the confinement options and the shared
/// namespaces in one place, and a diff of that node is the answer to "did that get worse".
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ContainerSecurity {
    /// Recorded even when false, because it is the field an auditor reads first and its
    /// absence would be indistinguishable from a facet that does not report it.
    pub privileged: bool,
    pub read_only_root_filesystem: bool,
    pub capabilities: ContainerCapabilities,
    /// The confinement the engine ended up applying: seccomp, apparmor, selinux labels,
    /// `no-new-privileges`.
    ///
    /// **The effective list, which is not the list that was asked for.** Measured on docker
    /// 26.1.5: a container given `--security-opt no-new-privileges` *and* `--pid host` comes
    /// back with `label=disable` as well, which docker added itself because sharing the
    /// host's pid namespace makes SELinux labelling impossible. The added option is the
    /// interesting one, and only the effective list has it.
    pub options: Vec<NonEmptyText>,
    pub namespaces: ContainerNamespaces,
}

impl From<&ContainerSecurity> for Observation {
    fn from(security: &ContainerSecurity) -> Self {
        Observation::object([
            ("capabilities", Observation::from(&security.capabilities)),
            ("namespaces", Observation::from(&security.namespaces)),
            (
                "options",
                Observation::list(
                    security
                        .options
                        .iter()
                        .map(|option| Observation::text(option.as_str())),
                ),
            ),
            ("privileged", Observation::boolean(security.privileged)),
            (
                "read_only_root_filesystem",
                Observation::boolean(security.read_only_root_filesystem),
            ),
        ])
    }
}
