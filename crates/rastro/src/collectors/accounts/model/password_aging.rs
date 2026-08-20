//! The policy the account database holds about when a password must change.

use rastro_collector::{CollectionError, Observation};

/// One account's password-ageing policy.
///
/// Six numbers, every one of which may be unset, and unset is not zero: a maximum
/// age of zero would force a change at every login, while an unset one imposes no
/// expiry at all. So each is an `Option` and each renders as `null` when the column
/// is blank.
///
/// **Two of the six are dates and four are durations**, which the field names carry
/// because nothing in the value does. A date is stored as whole days since the Unix
/// epoch, exactly as the file spells it, and is left that way rather than rendered
/// as a calendar date: the conversion is arithmetic rastro would have to be right
/// about for every historical leap second rule, and it buys readability rather than
/// signal. The raw number diffs just as well.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PasswordAging {
    pub last_changed_days_since_epoch: Option<i64>,
    pub minimum_age_days: Option<i64>,
    pub maximum_age_days: Option<i64>,
    pub warning_days: Option<i64>,
    pub inactive_days: Option<i64>,
    pub expires_days_since_epoch: Option<i64>,
}

/// Reads one ageing column, where blank means the policy is not set.
///
/// Signed on purpose, and `-1` is the reason: an empty column is how `chage` spells
/// "unset", but several tools write `-1` for the same thing, and refusing it would
/// turn an ordinary account into a failed facet. A misread cannot hide behind that
/// leniency, because the columns this parses would hold a name or a path if the line
/// had been tokenised wrongly, and neither parses as a number at all.
pub fn optional_days(column: &str) -> Result<Option<i64>, CollectionError> {
    if column.is_empty() {
        return Ok(None);
    }

    column
        .parse::<i64>()
        .map(Some)
        .map_err(|_| CollectionError::new(format!("{column:?} is not a number of days")))
}

impl From<&PasswordAging> for Observation {
    fn from(aging: &PasswordAging) -> Self {
        Observation::object([
            (
                "expires_days_since_epoch",
                days(aging.expires_days_since_epoch),
            ),
            ("inactive_days", days(aging.inactive_days)),
            (
                "last_changed_days_since_epoch",
                days(aging.last_changed_days_since_epoch),
            ),
            ("maximum_age_days", days(aging.maximum_age_days)),
            ("minimum_age_days", days(aging.minimum_age_days)),
            ("warning_days", days(aging.warning_days)),
        ])
    }
}

fn days(value: Option<i64>) -> Observation {
    value.map_or_else(Observation::null, Observation::integer)
}
