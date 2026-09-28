//! What a server said, once the framing is gone.

use rastro_collector::CollectionError;
use redis_protocol::resp2::types::OwnedFrame;

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

impl TryFrom<OwnedFrame> for Reply {
    type Error = CollectionError;

    fn try_from(frame: OwnedFrame) -> Result<Self, CollectionError> {
        Ok(match frame {
            OwnedFrame::SimpleString(bytes) => Reply::Simple(text_of(bytes)?),
            OwnedFrame::Error(message) => Reply::Error(message),
            OwnedFrame::Integer(number) => Reply::Integer(number),
            OwnedFrame::BulkString(bytes) => Reply::Bulk(text_of(bytes)?),
            OwnedFrame::Null => Reply::Nil,
            OwnedFrame::Array(frames) => Reply::Array(
                frames
                    .into_iter()
                    .map(Reply::try_from)
                    .collect::<Result<_, _>>()?,
            ),
        })
    }
}

fn text_of(bytes: Vec<u8>) -> Result<String, CollectionError> {
    String::from_utf8(bytes)
        .map_err(|_| CollectionError::new("the server replied with bytes that are not valid UTF-8"))
}
