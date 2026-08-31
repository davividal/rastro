//! A second count as a calendar instant, for the name of a file.

/// Seconds since the epoch as `YYYYMMDDTHHMMSSZ`, in UTC.
///
/// **UTC and no colon**, because this ends up in a filename: a colon needs shell quoting,
/// breaks on VFAT and exFAT, and reads as a host separator to `scp` and `rsync`. Sortable
/// because a directory of these is meant to be listed in order.
///
/// **Hand-rolled rather than a dependency.** `jiff` or `time` would bring a calendar, a
/// timezone database and a parser to format one integer, into a binary whose whole supply
/// chain is meant to be auditable. This is twenty lines of integer arithmetic with no
/// configuration and no locale.
pub fn utc_stamp(seconds_since_epoch: i64) -> String {
    let days = seconds_since_epoch.div_euclid(86_400);
    let within_day = seconds_since_epoch.rem_euclid(86_400);

    let (year, month, day) = civil_from_days(days);
    let hour = within_day / 3_600;
    let minute = (within_day % 3_600) / 60;
    let second = within_day % 60;

    format!("{year:04}{month:02}{day:02}T{hour:02}{minute:02}{second:02}Z")
}

/// The civil date a day count lands on, by Howard Hinnant's `civil_from_days`.
///
/// The algorithm shifts the year to start in March so that the leap day is the last day of
/// it, which is what removes every special case: no branch on February, and the 400-year
/// Gregorian cycle (146,097 days) divides evenly. Correct for any year `i64` can hold, so
/// 2038 is not a boundary here and 2100 not being a leap year needs no rule of its own.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    // Shifted to an era beginning 0000-03-01, which is 719,468 days before the epoch.
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);

    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);

    // `month_of_year` counts from March, so January and February belong to the next year.
    let month_of_year = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_of_year + 2) / 5 + 1;
    let month = match month_of_year < 10 {
        true => month_of_year + 3,
        false => month_of_year - 9,
    };

    (year + i64::from(month <= 2), month, day)
}
