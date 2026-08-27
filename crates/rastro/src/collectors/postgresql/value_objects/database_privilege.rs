//! What a grant on a database permits.

use rastro_collector::CollectionError;

/// One of the three privileges a database can be granted.
///
/// Closed, because the server has exactly three at the database level and each is a
/// different thing an operator decides. A name this does not know means the server gained a
/// fourth, and that is a schema question rather than something to guess at: `CREATE` is what
/// `CREATE SCHEMA` is checked against, `CONNECT` is what every login rests on, and
/// `TEMPORARY` is neither.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DatabasePrivilege {
    Connect,
    Create,
    Temporary,
}

impl DatabasePrivilege {
    /// Reads the name `aclexplode` returns.
    ///
    /// The spelled-out name rather than the letter of an `aclitem`, because the rows are
    /// what the collector asks for: `C` and `c` differ only in case, and a mis-cased letter
    /// would silently swap CREATE for CONNECT.
    pub fn of(value: &str) -> Result<Self, CollectionError> {
        match value {
            "CONNECT" => Ok(Self::Connect),
            "CREATE" => Ok(Self::Create),
            "TEMPORARY" => Ok(Self::Temporary),
            other => Err(CollectionError::new(format!(
                "{other:?} is not a database privilege postgres had when this was written, so \
                 what it grants cannot be told"
            ))),
        }
    }

    /// The name the document records, spelled as `GRANT` spells it.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Connect => "CONNECT",
            Self::Create => "CREATE",
            Self::Temporary => "TEMPORARY",
        }
    }
}
