//! The crontab grammar.
//!
//! Two dialects of one format, and the difference is a single column. A *system* crontab —
//! `/etc/crontab` and every file in `/etc/cron.d` — puts the account between the schedule and
//! the command. A *user* crontab out of the spool does not, because the account is the file's
//! name and a user cannot choose it. Reading one as the other silently turns the first word of
//! a command into an account name.

use rastro_collector::CollectionError;

use crate::collectors::cron::model::{CronJob, CronTable};
use crate::collectors::cron::value_objects::{CronCommand, JobOwner, Schedule, VariableName};

/// How many fields a numeric schedule has: minute, hour, day of month, month, day of week.
const SCHEDULE_FIELDS: usize = 5;

/// What introduces one of cron's shorthand schedules.
const SHORTHAND: char = '@';

/// Whether the dialect being read carries an account column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnerColumn {
    /// `/etc/crontab` and `/etc/cron.d/*`.
    Present,
    /// A crontab out of `/var/spool/cron/crontabs`.
    Absent,
}

/// Translates one crontab into the model.
pub fn parse(contents: &str, owner_column: OwnerColumn) -> Result<CronTable, CollectionError> {
    let mut environment = Vec::new();
    let mut jobs = Vec::new();

    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        match assignment(line) {
            Some((name, value)) => environment.push((VariableName::new(name)?, value.to_owned())),
            None => jobs.push(parse_job(line, owner_column)?),
        }
    }

    CronTable::crontab(environment, jobs)
}

/// Whether a line sets a variable rather than scheduling a job.
///
/// **The check is on the shape of the name, not on the presence of a `=`.** A command may
/// contain one — `SERVICE_MODE=1 /sbin/e2scrub_all` is a real command on this box, and
/// `5-55/10 * * * * root command -v x` contains none — so splitting on `=` would take a job for
/// an assignment. A schedule's first field always begins with a digit, a `*`, or an `@`, and an
/// assignment's name never does.
fn assignment(line: &str) -> Option<(&str, &str)> {
    let (name, value) = line.split_once('=')?;
    let name = name.trim();

    let first = name.chars().next()?;
    if !(first.is_ascii_alphabetic() || first == '_') {
        return None;
    }
    if !name
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return None;
    }

    Some((name, value.trim()))
}

fn parse_job(line: &str, owner_column: OwnerColumn) -> Result<CronJob, CollectionError> {
    let (schedule, rest) = split_schedule(line)?;

    let (owner, command) = match owner_column {
        OwnerColumn::Absent => (None, rest),
        OwnerColumn::Present => {
            let (owner, command) = rest.split_once(char::is_whitespace).ok_or_else(|| {
                CollectionError::new(format!(
                    "a system crontab line needs an account and a command after its \
                     schedule: {line:?}"
                ))
            })?;
            (Some(JobOwner::new(owner)?), command)
        }
    };

    Ok(CronJob {
        schedule,
        owner,
        command: CronCommand::new(command.trim())?,
    })
}

/// Peels the schedule off the front of a line.
///
/// A shorthand is one word; a numeric schedule is exactly five, however much whitespace
/// separates them. Taking five *fields* rather than a fixed slice is what makes the
/// tab-aligned crontabs Debian ships parse the same as space-separated ones.
fn split_schedule(line: &str) -> Result<(Schedule, &str), CollectionError> {
    if line.starts_with(SHORTHAND) {
        let (shorthand, rest) = line.split_once(char::is_whitespace).ok_or_else(|| {
            CollectionError::new(format!("{line:?} is a schedule with no command after it"))
        })?;

        return Ok((Schedule::new(shorthand)?, rest.trim_start()));
    }

    let mut remainder = line;
    for _ in 0..SCHEDULE_FIELDS {
        let (_, rest) = remainder.split_once(char::is_whitespace).ok_or_else(|| {
            CollectionError::new(format!(
                "expected {SCHEDULE_FIELDS} schedule fields and a command: {line:?}"
            ))
        })?;
        remainder = rest.trim_start();
    }

    let schedule = &line[..line.len() - remainder.len()];

    Ok((Schedule::new(schedule)?, remainder))
}
