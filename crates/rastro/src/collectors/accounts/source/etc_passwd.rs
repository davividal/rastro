//! The `/etc/passwd` interface.
//!
//! The file's spelling, kept apart from rastro's meaning. Everything peculiar to
//! this interface lives here: seven colon-separated columns, blank columns that mean
//! "the kernel's default" rather than nothing, and the compat directives that make
//! the file a pointer to another nameservice instead of an account list.

use rastro_collector::{AbsolutePath, CollectionError};

use crate::collectors::accounts::value_objects::{Comment, GroupId, UserId, UserName};

/// How many columns a passwd line has.
const COLUMNS: usize = 7;

/// One line of the file, translated but not yet joined to the shadow database.
///
/// Separate from `UserAccount` because a passwd line cannot answer everything a user
/// account knows: whether the account has a password lives in another file, and a
/// type that carried an always-`None` field would invite a reader to think the file
/// had been consulted and found nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasswdEntry {
    pub name: UserName,
    pub user_id: UserId,
    pub primary_group_id: GroupId,
    pub comment: Comment,
    pub home_directory: Option<AbsolutePath>,
    pub login_shell: Option<AbsolutePath>,
}

/// The characters that introduce a compat directive rather than an account.
///
/// `+` and `-` at the start of the name column are not names. They tell glibc's
/// `compat` nameservice to pull accounts in from NIS or LDAP, or to blank one out
/// again, and a line like `+::::::` means "and everything the directory server
/// says". Recording that as a user called `+` would be nonsense; ignoring it would
/// be worse, because it means the local file is a fragment of the account database
/// rather than the whole of it.
const COMPAT_MARKERS: [char; 2] = ['+', '-'];

/// The local account list as a source rastro can read.
pub struct EtcPasswd;

impl EtcPasswd {
    /// Translates the file's text into entries.
    ///
    /// **Blank lines and `#` comments are skipped, because glibc skips them.** The
    /// rule is not rastro's invention: `files-parse.c` in glibc drops both before
    /// parsing, so a commented-out account genuinely is not an account, and treating
    /// one as a parse failure would refuse a file the host is perfectly happy with.
    ///
    /// A line with content that does not parse is a failure, never a skip.
    pub fn parse(text: &str) -> Result<Vec<PasswdEntry>, CollectionError> {
        text.lines()
            .filter(|line| carries_an_entry(line))
            .map(Self::parse_line)
            .collect()
    }

    fn parse_line(line: &str) -> Result<PasswdEntry, CollectionError> {
        let columns: Vec<&str> = line.split(':').collect();
        let [
            name,
            _password,
            user_id,
            group_id,
            comment,
            home_directory,
            login_shell,
        ] = columns.as_slice()
        else {
            return Err(CollectionError::new(format!(
                "expected {COLUMNS} colon-separated columns in an /etc/passwd line, got {}: \
                 {line:?}",
                columns.len()
            )));
        };

        refuse_a_compat_directive(name)?;

        Ok(PasswdEntry {
            name: UserName::new(*name)?,
            user_id: UserId::parse(user_id)?,
            primary_group_id: GroupId::parse(group_id)?,
            comment: Comment::new(*comment),
            home_directory: optional_path(home_directory, "home directory")?,
            login_shell: optional_path(login_shell, "login shell")?,
        })
    }
}

/// Whether a line is an account rather than blank space or a comment.
pub fn carries_an_entry(line: &str) -> bool {
    !line.trim().is_empty() && !line.starts_with('#')
}

/// Refuses a line that delegates accounts to another nameservice.
///
/// **Refused rather than skipped, and the difference is the whole point.** A host
/// with a `+` line has accounts that are in `getent passwd` and in no file rastro
/// can read, so the list rastro assembled is not the host's account database. This
/// project's one unacceptable failure is reporting something it half-understood as
/// complete, so the facet fails with a reason an operator can act on instead of
/// quietly under-reporting who can log in.
pub fn refuse_a_compat_directive(name: &str) -> Result<(), CollectionError> {
    if name.starts_with(COMPAT_MARKERS) {
        return Err(CollectionError::new(format!(
            "{name:?} is a compat directive, so this host takes accounts from a directory \
             service as well and the local files are not the whole account database"
        )));
    }

    Ok(())
}

/// A path column, where blank means the file said nothing.
///
/// Blank is not turned into the default the kernel would apply. `/bin/sh` for an
/// empty shell and `/` for an empty home are substitutions made at login time, and
/// writing them here would record a value the file does not contain.
fn optional_path(column: &str, kind: &str) -> Result<Option<AbsolutePath>, CollectionError> {
    if column.is_empty() {
        return Ok(None);
    }

    Ok(Some(AbsolutePath::new(column, kind)?))
}
