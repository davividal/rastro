//! Where the password a server was started with can be read, and only there.

use std::path::Path;

use super::config_password::requirepass_in;
use super::server_unit::{start_of, unit_of};
use crate::collectors::canonical_tool::CanonicalTool;

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
    let unit = unit_of(proc, process_id).map_err(|reason| {
        format!(
            "the server requires a password, and {reason}, so which file it was started with is \
             unknown"
        )
    })?;
    let systemctl = systemctl.ok_or_else(|| {
        format!(
            "the server requires a password, and there is no systemctl to ask how {unit} starts it"
        )
    })?;
    let start = start_of(systemctl, &unit).map_err(|error| {
        format!(
            "the server requires a password, and how {unit} starts it could not be read: {error}"
        )
    })?;

    if let Some(password) = start.password {
        return Ok(Credential {
            password,
            origin: format!("the command line of {unit}"),
        });
    }

    let file = start.config_file.ok_or_else(|| {
        format!("the server requires a password, and {unit} starts it with no configuration file")
    })?;
    let origin = file.display().to_string();

    match requirepass_in(&file) {
        Ok(Some(password)) => Ok(Credential { password, origin }),
        Ok(None) => Err(format!(
            "the server requires a password and {origin} sets no password, so it was set at \
             runtime, where rastro has no way to read it"
        )),
        Err(error) => Err(format!("the server requires a password, and {error}")),
    }
}
