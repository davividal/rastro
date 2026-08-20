//! The `sshd -T` interface.
//!
//! `sshd -T` prints every setting with every drop-in and `Match` default already applied, one
//! `name value` pair per line, with the names lower-cased. It is OpenSSH's own
//! configuration dump, so this is the effective-state source `design.md` asks for rather than
//! a read of `sshd_config` and its includes.

use std::collections::BTreeMap;

use rastro_collector::CollectionError;

use crate::collectors::canonical_tool::CanonicalTool;
use crate::collectors::ssh_access::model::SshServer;
use crate::collectors::ssh_access::value_objects::SettingValue;

const PROGRAM: &str = "sshd";

/// Dump the effective configuration and exit.
const TEST_MODE: &str = "-T";

const PERMIT_ROOT_LOGIN: &str = "permitrootlogin";
const PASSWORD_AUTHENTICATION: &str = "passwordauthentication";
const PUBLIC_KEY_AUTHENTICATION: &str = "pubkeyauthentication";
const AUTHORIZED_KEYS_FILE: &str = "authorizedkeysfile";
const AUTHORIZED_KEYS_COMMAND: &str = "authorizedkeyscommand";

/// The ssh server's effective configuration, as a source rastro can read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sshd {
    tool: CanonicalTool,
}

impl Sshd {
    /// Finds `sshd`, or reports that this host does not run one.
    pub fn detect() -> Option<Self> {
        CanonicalTool::located(PROGRAM).map(Self::using)
    }

    /// The same over a tool the caller located.
    pub fn using(tool: CanonicalTool) -> Self {
        Self { tool }
    }

    pub fn tool(&self) -> &CanonicalTool {
        &self.tool
    }

    pub fn read(&self) -> Result<SshServer, CollectionError> {
        Self::parse(&self.tool.run(&[TEST_MODE])?)
    }

    /// Translates the dump into the model.
    ///
    /// Separate from [`Self::read`] so the whole translation is exercised from a fixture, with
    /// no sshd to run.
    ///
    /// **Every setting read here is required.** `sshd -T` prints all of them unconditionally
    /// because it prints its own defaults, so a missing one means the output is not what rastro
    /// believes. Defaulting `passwordauthentication` would be a claim about whether the box
    /// accepts passwords.
    pub fn parse(dump: &str) -> Result<SshServer, CollectionError> {
        let settings: BTreeMap<&str, &str> = dump
            .lines()
            .filter_map(|line| line.trim().split_once(char::is_whitespace))
            .map(|(name, value)| (name.trim(), value.trim()))
            .collect();

        Ok(SshServer {
            permit_root_login: setting(&settings, PERMIT_ROOT_LOGIN)?,
            password_authentication: setting(&settings, PASSWORD_AUTHENTICATION)?,
            public_key_authentication: setting(&settings, PUBLIC_KEY_AUTHENTICATION)?,
            // Whitespace-separated, and the order is the order sshd searches them in.
            authorized_keys_files: field(&settings, AUTHORIZED_KEYS_FILE)?
                .split_whitespace()
                .map(str::to_owned)
                .collect(),
            authorized_keys_command: setting(&settings, AUTHORIZED_KEYS_COMMAND)?,
        })
    }
}

fn field<'a>(
    settings: &BTreeMap<&'a str, &'a str>,
    name: &str,
) -> Result<&'a str, CollectionError> {
    settings.get(name).copied().ok_or_else(|| {
        CollectionError::new(format!(
            "`{PROGRAM} {TEST_MODE}` reported no {name:?} setting"
        ))
    })
}

fn setting(settings: &BTreeMap<&str, &str>, name: &str) -> Result<SettingValue, CollectionError> {
    SettingValue::new(field(settings, name)?)
}
