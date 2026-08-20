//! Whether an account can be logged into with a password, and how.

use rastro_collector::Observation;

use crate::collectors::accounts::value_objects::HashAlgorithm;

/// What stands in an account's password field.
///
/// **Four states, because the field really does mean four different things**, and
/// collapsing them would hide the one change an operator most needs to see. The
/// difference between an account with no password set and an account that needs no
/// password to log in is the difference between a locked door and an open one, and
/// both are spelled with a short string in the same column.
///
/// Rendered as a composed node rather than a single token: the state and the scheme
/// are two facts, and fusing them into `"usable:y"` would make a reader parse a
/// value rather than read it.
///
/// **What this type deliberately cannot express: that a password changed.** It has no
/// variant and no field that holds a hash, so re-running `passwd` on an account
/// leaves every value here identical. The state stays `Usable`, the scheme stays the
/// same, and the diff is empty. The date in
/// [`PasswordAging`](super::PasswordAging) is what moves instead. The collector's own
/// documentation states the limitation in full; it is repeated here because this is
/// the type a reader lands on when they ask what rastro knows about a password.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PasswordStatus {
    /// The field is empty, so this account authenticates with no password at all.
    ///
    /// The most consequential value the column can hold, and the easiest to miss:
    /// nothing about an empty string looks alarming in a file full of `*`.
    Absent,

    /// A placeholder stands where a hash would, so no password can ever match.
    ///
    /// The marker is kept as written because different tools write different ones
    /// and they are not synonyms in practice: Debian's `adduser` leaves `*` on a
    /// system account that never had a password, while `useradd` leaves `!!`, and
    /// telling those apart is how an operator knows which tool made the account.
    Unusable { marker: String },

    /// A hash is present, prefixed with `!`, so logins against it are refused.
    ///
    /// This is `passwd -l`, and it is not the same as [`Self::Unusable`]: the
    /// password is still there and `passwd -u` brings it back.
    ///
    /// The distinction is easy to get wrong, and was got wrong while this was being
    /// written. A first pass over the development box counted every field beginning
    /// with `!` as one of these and found five. Four of them are `!*`, which is a
    /// locked account that never had a password at all, and only one is a locked
    /// hash. Stripping the marker and then asking whether what remains is a hash is
    /// what tells the two apart; `starts_with('!')` does not.
    Locked { algorithm: Option<HashAlgorithm> },

    /// A hash a login can match.
    Usable { algorithm: Option<HashAlgorithm> },
}

/// The character that introduces a crypt hash's algorithm identifier.
const ALGORITHM_MARKER: char = '$';

/// What a locked password field is prefixed with.
const LOCK_MARKER: char = '!';

impl PasswordStatus {
    /// Reads the field, without ever keeping the hash it may contain.
    ///
    /// The parse order matters and is not arbitrary. Emptiness comes first because
    /// an empty string is a prefix of everything. The lock marker is stripped next,
    /// so that `!` alone falls through to a placeholder while `!$y$...` is
    /// recognised as a locked hash rather than as a placeholder called `!$y$...`.
    pub fn parse(field: &str) -> Self {
        if field.is_empty() {
            return Self::Absent;
        }

        let (locked, hash) = match field.strip_prefix(LOCK_MARKER) {
            Some(rest) => (true, rest),
            None => (false, field),
        };

        if !hash.starts_with(ALGORITHM_MARKER) && !looks_like_legacy_crypt(hash) {
            return Self::Unusable {
                marker: field.to_owned(),
            };
        }

        let algorithm = algorithm_of(hash);
        if locked {
            return Self::Locked { algorithm };
        }

        Self::Usable { algorithm }
    }

    /// The word this state is recorded under.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Absent => "absent",
            Self::Unusable { .. } => "unusable",
            Self::Locked { .. } => "locked",
            Self::Usable { .. } => "usable",
        }
    }
}

/// The length of a traditional DES `crypt` hash.
///
/// Thirteen characters of the hash alphabet and no `$` anywhere, which is what a
/// password set before 1999 still looks like. Recognising it is what stops such an
/// account being reported as an unusable placeholder, which would read as "nobody
/// can log in here" about an account where somebody can.
const LEGACY_CRYPT_LENGTH: usize = 13;

fn looks_like_legacy_crypt(hash: &str) -> bool {
    hash.len() == LEGACY_CRYPT_LENGTH
        && hash.bytes().all(|character| {
            character.is_ascii_alphanumeric() || character == b'.' || character == b'/'
        })
}

/// The identifier between the first two `$`, if the hash carries one.
///
/// A legacy DES hash carries none, which is why this is an `Option` rather than a
/// failure: the account is perfectly usable and rastro simply has no scheme name to
/// report for it.
fn algorithm_of(hash: &str) -> Option<HashAlgorithm> {
    let identifier = hash
        .strip_prefix(ALGORITHM_MARKER)?
        .split(ALGORITHM_MARKER)
        .next()?;

    HashAlgorithm::new(identifier).ok()
}

impl From<&PasswordStatus> for Observation {
    fn from(status: &PasswordStatus) -> Self {
        let mut entries = vec![("state", Observation::text(status.as_str()))];

        match status {
            PasswordStatus::Unusable { marker } => {
                entries.push(("marker", Observation::text(marker.clone())));
            }
            PasswordStatus::Locked { algorithm } | PasswordStatus::Usable { algorithm } => {
                entries.push((
                    "algorithm",
                    algorithm
                        .as_ref()
                        .map_or_else(Observation::null, Observation::from),
                ));
            }
            PasswordStatus::Absent => {}
        }

        Observation::object(entries)
    }
}
