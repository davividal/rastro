//! The account a tool is run as.

use rastro_collector::CollectionError;

/// A user name a tool can be delegated to.
///
/// sudo's own word for it, and validated because the value reaches sudo as an argument
/// and does not always come from a literal: a cluster's owner is read from the host.
///
/// **Two prefixes are the reason this is a type**, and both are sudo reading the operand
/// as something other than an account name. A leading dash makes `-u` or
/// `--reset-timestamp` an option; a leading `#` makes `#0` a numeric UID, so the account
/// rastro means to drop to becomes root and the drop does not happen. Together they are the
/// way an argument vector can still be turned against the tool that receives it. Nothing
/// here needs quoting, because rastro never builds a command line; this closes the gap that
/// remains once quoting is out of the picture.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct TargetUser(String);

impl TargetUser {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        let name = value.into();

        if name.is_empty() {
            return Err(CollectionError::new(
                "a target user name cannot be empty, and no account has an empty name",
            ));
        }

        if name.starts_with('-') {
            return Err(CollectionError::new(format!(
                "{name:?} cannot be a target user, because sudo would read it as an option \
                 rather than as an account"
            )));
        }

        if name.starts_with('#') {
            return Err(CollectionError::new(format!(
                "{name:?} cannot be a target user, because sudo would read it as a numeric \
                 user ID rather than as an account, and {name:?} could name root"
            )));
        }

        if name.chars().any(char::is_whitespace) {
            return Err(CollectionError::new(format!(
                "{name:?} cannot be a target user, because no account name contains whitespace"
            )));
        }

        Ok(Self(name))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
