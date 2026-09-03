//! One instruction in a configuration.

use crate::collectors::nginx::value_objects::{DirectiveArgument, DirectiveName};

/// A directive, and the block it opens if it opens one.
///
/// **No line number, on purpose.** The parser knows where every directive sits and says so
/// when it refuses a file, but a line number in the document would make a comment added at
/// the top of a file read as a change to every directive under it. What changed is the
/// directive, not where it lives.
///
/// `block` tells `location /x { }` from `location /x;`: an empty block is `Some` and no
/// block at all is `None`. nginx accepts both spellings and they are not the same
/// configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Directive {
    pub name: DirectiveName,
    pub arguments: Vec<DirectiveArgument>,
    pub block: Option<Vec<Directive>>,
}
