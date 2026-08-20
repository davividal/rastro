//! How the server is configured to let anybody in.

use rastro_collector::Observation;

use crate::collectors::ssh_access::value_objects::SettingValue;

/// The sshd settings that decide whether a key matters at all.
///
/// **Collected alongside the keys because either half is misleading without the other.** A
/// hundred authorized keys mean nothing if `PubkeyAuthentication` is `no`, and an empty
/// `authorized_keys` file means nothing if `PasswordAuthentication` is `yes`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshServer {
    pub permit_root_login: SettingValue,
    pub password_authentication: SettingValue,
    pub public_key_authentication: SettingValue,
    /// The patterns sshd resolves a user's key files from, in its own order, which is the
    /// order it searches them in.
    pub authorized_keys_files: Vec<String>,
    /// **When this is not `none`, the file list below is not the whole answer**: sshd asks a
    /// program for a user's keys, and that program's output is not on the filesystem for
    /// rastro to read. Recording the setting is how the facet says so rather than quietly
    /// under-reporting who can log in.
    pub authorized_keys_command: SettingValue,
}

impl From<&SshServer> for Observation {
    fn from(server: &SshServer) -> Self {
        Observation::object([
            (
                "authorized_keys_command",
                Observation::from(&server.authorized_keys_command),
            ),
            (
                "authorized_keys_files",
                Observation::list(
                    server
                        .authorized_keys_files
                        .iter()
                        .map(|pattern| Observation::text(pattern.clone())),
                ),
            ),
            (
                "password_authentication",
                Observation::from(&server.password_authentication),
            ),
            (
                "permit_root_login",
                Observation::from(&server.permit_root_login),
            ),
            (
                "public_key_authentication",
                Observation::from(&server.public_key_authentication),
            ),
        ])
    }
}
