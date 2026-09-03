//! One switch the binary was built with.

use rastro_collector::{CollectionError, NonEmptyText, Observation};

/// A single entry of the `configure arguments:` line.
///
/// **This is where the modules are.** nginx has no runtime module list to ask for, so
/// `--with-http_v2_module` and `--add-module=...` in this list are the only account of what
/// the binary can do, and a rebuilt package that quietly dropped one would show here and
/// nowhere else.
///
/// Kept in the order the binary printed, which is the order it was configured with, and
/// unquoted the way a directive's arguments are: `--with-cc-opt='-g -O2'` holds spaces
/// inside one argument, and the quotes are the shell's spelling rather than part of the
/// value.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ConfigureArgument(NonEmptyText);

impl ConfigureArgument {
    pub fn new(value: impl Into<String>) -> Result<Self, CollectionError> {
        Ok(Self(NonEmptyText::new(value, "configure argument")?))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<&ConfigureArgument> for Observation {
    fn from(argument: &ConfigureArgument) -> Self {
        Observation::text(argument.as_str())
    }
}
