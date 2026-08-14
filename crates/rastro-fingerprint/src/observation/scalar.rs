//! The values an observation can bottom out in.

/// A leaf value.
///
/// Floating point is deliberately absent. Rendering a float back to text is not
/// reliably identical across platforms and library versions, and a
/// byte-identical diffable view is the contract the whole tool rests on.
/// Collectors with fractional data emit a scaled integer or text. See
/// `docs/decisions.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scalar {
    Null,
    Boolean(bool),
    Integer(i64),
    Text(String),
}
