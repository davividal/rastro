//! What a server said, once the framing is gone.

use rastro_collector::CollectionError;
use redis::Value;

/// One reply, in the shapes RESP2 has.
///
/// **An error reply is a reply**, not a failure of the read: `NOAUTH` says the server wants a
/// password and `ERR unknown command` says an operator renamed `CONFIG` away, and the caller has
/// to tell those apart to say the right thing about the box.
///
/// **Text is strictly UTF-8.** A bulk string is bytes on the wire, and one that does not decode
/// is refused rather than repaired, for the reason the canonical tool seam refuses it: a
/// replacement character is text that was never on the box.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reply {
    Simple(String),
    Error(String),
    Integer(i64),
    Bulk(String),
    /// RESP2 spells a missing value and a missing array the same way, and so does this.
    Nil,
    Array(Vec<Reply>),
}

impl TryFrom<Value> for Reply {
    type Error = CollectionError;

    /// The reply in rastro's terms, or a refusal for a shape RESP2 does not have.
    ///
    /// Nothing here sends `HELLO 3`, so a map, a set, a double or a push is a server answering in a
    /// protocol rastro did not ask for. Recursion is safe: the parser refuses nesting past a
    /// hundred levels before this sees it, and real replies nest three deep.
    fn try_from(value: Value) -> Result<Self, CollectionError> {
        Ok(match value {
            Value::SimpleString(text) => Reply::Simple(text),
            Value::Okay => Reply::Simple("OK".to_owned()),
            Value::ServerError(error) => Reply::Error(match error.details() {
                Some(details) => format!("{} {details}", error.code()),
                None => error.code().to_owned(),
            }),
            Value::Int(number) => Reply::Integer(number),
            Value::BulkString(bytes) => Reply::Bulk(text_of(bytes)?),
            Value::Nil => Reply::Nil,
            Value::Array(values) => Reply::Array(
                values
                    .into_iter()
                    .map(Reply::try_from)
                    .collect::<Result<_, _>>()?,
            ),
            _ => {
                return Err(CollectionError::new(
                    "the server replied in a protocol newer than the RESP2 rastro asked in",
                ));
            }
        })
    }
}

fn text_of(bytes: Vec<u8>) -> Result<String, CollectionError> {
    String::from_utf8(bytes)
        .map_err(|_| CollectionError::new("the server replied with bytes that are not valid UTF-8"))
}
