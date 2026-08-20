//! The `/etc/group` interface.

use rastro_collector::CollectionError;

use super::etc_passwd::{carries_an_entry, refuse_a_compat_directive};
use crate::collectors::accounts::model::GroupAccount;
use crate::collectors::accounts::value_objects::{GroupId, GroupMembers, GroupName, UserName};

/// How many columns a group line has.
const COLUMNS: usize = 4;

/// The local group list as a source rastro can read.
pub struct EtcGroup;

impl EtcGroup {
    /// Translates the file's text into the model.
    ///
    /// Blank lines and comments are skipped and compat directives refused, on the
    /// same reasoning as the passwd file's, which is why both rules are imported
    /// from there rather than restated: the two files are read by the same glibc
    /// parser and delegate to the same nameservice.
    pub fn parse(text: &str) -> Result<Vec<(GroupName, GroupAccount)>, CollectionError> {
        text.lines()
            .filter(|line| carries_an_entry(line))
            .map(Self::parse_line)
            .collect()
    }

    fn parse_line(line: &str) -> Result<(GroupName, GroupAccount), CollectionError> {
        let columns: Vec<&str> = line.split(':').collect();
        let [name, _password, group_id, members] = columns.as_slice() else {
            return Err(CollectionError::new(format!(
                "expected {COLUMNS} colon-separated columns in an /etc/group line, got {}: \
                 {line:?}",
                columns.len()
            )));
        };

        refuse_a_compat_directive(name)?;

        let group = GroupAccount {
            group_id: GroupId::parse(group_id)?,
            members: parse_members(members)?,
        };

        Ok((GroupName::new(*name)?, group))
    }
}

/// Splits the member column, where an empty column means no members at all.
///
/// The emptiness check has to come before the split, because splitting `""` on a
/// comma yields one empty name rather than no names, and every group on an ordinary
/// box has an empty member column. Without it, a hundred groups would each acquire
/// one nameless member and the facet would fail on the first of them.
fn parse_members(column: &str) -> Result<GroupMembers, CollectionError> {
    if column.is_empty() {
        return Ok(GroupMembers::default());
    }

    let members = column
        .split(',')
        .map(UserName::new)
        .collect::<Result<Vec<UserName>, CollectionError>>()?;

    Ok(GroupMembers::new(members))
}
