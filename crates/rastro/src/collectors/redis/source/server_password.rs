//! Where the password a server was started with can be read, and only there.

use std::path::Path;

use super::config_password::{
    PasswordDirectives, default_user_in_acl_file, password_directives_in,
};
use super::default_account::password_for_default_account;
use super::server_unit::{start_of, unit_of};
use crate::collectors::canonical_tool::CanonicalTool;

/// The rules an ACL file without a `default` line leaves that account with, measured.
const SWITCHED_ON: &str = "on";
const NO_PASSWORD: &str = "nopass";

/// A password, and where it was read from, for a message that must never quote it.
pub struct Credential {
    pub password: String,
    pub origin: String,
}

/// The password the server behind `process_id` was started with, or why it cannot be known.
///
/// **Every route is the server's own start, never a default.** Assuming `/etc/redis/redis.conf`
/// for a server started from another file would send it that file's password, and a wrong one is
/// an entry in its `ACL LOG`. So the unit is read from the process, the file from the unit, and
/// the password from the file, and a break anywhere is said rather than guessed across.
///
/// **A password set with `CONFIG SET` and written nowhere is unreachable**, and that is the box
/// telling the truth about how it was provisioned rather than a gap to be filled.
pub fn password_for(
    proc: &Path,
    process_id: u32,
    systemctl: Option<&CanonicalTool>,
) -> Result<Credential, String> {
    let cgroup = unit_of(proc, process_id).map_err(|reason| {
        format!(
            "the server requires a password, and {reason}, so which file it was started with is \
             unknown"
        )
    })?;
    let systemctl = systemctl.ok_or_else(|| {
        format!(
            "the server requires a password, and there is no systemctl to ask how {} starts it",
            cgroup.unit
        )
    })?;
    let start = start_of(systemctl, &cgroup)
        .map_err(|error| format!("the server requires a password, and {error}"))?;
    let unit = cgroup.unit.as_str();

    let file = start.config_file;
    if file.is_none() && start.password.is_none() {
        return Err(format!(
            "the server requires a password, and {unit} starts it with no configuration file"
        ));
    }

    let directives = match &file {
        Some(file) => password_directives_in(file)
            .map_err(|error| format!("the server requires a password, and {error}"))?,
        None => PasswordDirectives::default(),
    };

    // Measured: with an ACL file the server ignores `requirepass` and the file's own `user`
    // lines, and an ACL file that declares no `default` leaves that account without a password.
    let (default_user, origin) = match &directives.acl_file {
        Some(acl_file) if acl_file.is_relative() => {
            return Err(format!(
                "the server requires a password, and its ACL file {} is named by a relative \
                 path, which the server resolved against a working directory nothing records",
                acl_file.display()
            ));
        }
        Some(acl_file) => {
            let rules = default_user_in_acl_file(acl_file)
                .map_err(|error| format!("the server requires a password, and {error}"))?
                .unwrap_or_else(|| vec![SWITCHED_ON.to_owned(), NO_PASSWORD.to_owned()]);
            (Some(rules), acl_file.display().to_string())
        }
        None => (
            directives.default_user,
            match &file {
                Some(file) => file.display().to_string(),
                None => format!("the command line of {unit}"),
            },
        ),
    };

    // The command line is applied after the file, so its `--requirepass` is the later one.
    let requirepass = start.password.or(directives.requirepass);
    let password = password_for_default_account(requirepass.as_deref(), default_user.as_deref())
        .map_err(|reason| format!("the server requires a password, and {reason} ({origin})"))?;

    Ok(Credential { password, origin })
}
