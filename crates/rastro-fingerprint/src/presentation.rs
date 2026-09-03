//! How much of a document is shown, on the two axes that decide it.
//!
//! `present` is the established verb for this side of the line: collectors
//! classify, renderers present. Both axes answer *what is in* the document
//! rather than what it looks like, so both live here and neither is a format.

use crate::view::View;

/// Whether a value a collector marked sensitive is shown as it stands.
///
/// **Redaction is not a view.** A volatile value is dropped from the diffable
/// view and kept in the complete one; a sensitive value is digested in *both*,
/// because the complete view is a fuller document and not a way round an
/// annotation. So this is a second axis, and `--raw` is its only opt-out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Disclosure {
    /// Sensitive values stand in as a digest. The default, and the reason a
    /// caller that has thought about neither axis still gets the safe one.
    #[default]
    Redacted,
    /// Sensitive values are shown as they stand, which is what `--raw` asks
    /// for.
    Raw,
}

/// Which values of a document are shown, and whether the sensitive ones stand
/// in as digests.
///
/// A named pair rather than two parameters, because the two travel together
/// everywhere and a call site reading `in_view(Complete, Raw)` says less than
/// the positions cost. It also makes the default structural: [`From<View>`]
/// fills in [`Disclosure::Redacted`], so a caller who has never heard of
/// redaction cannot accidentally opt out of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Presentation {
    view: View,
    disclosure: Disclosure,
}

impl Presentation {
    /// The view worth diffing, with sensitive values digested.
    pub fn diffable() -> Self {
        Self::from(View::Diffable)
    }

    /// Everything the collectors observed, with sensitive values digested.
    pub fn complete() -> Self {
        Self::from(View::Complete)
    }

    /// The same view, showing sensitive values as they stand.
    ///
    /// Consuming `self` rather than taking a flag: opting out of redaction is a
    /// deliberate act at the call site, and `Presentation::complete().raw()`
    /// reads as one.
    pub fn raw(self) -> Self {
        Self {
            disclosure: Disclosure::Raw,
            ..self
        }
    }

    pub fn view(&self) -> View {
        self.view
    }

    pub fn disclosure(&self) -> Disclosure {
        self.disclosure
    }
}

impl From<View> for Presentation {
    /// A view alone means the safe disclosure.
    fn from(view: View) -> Self {
        Self {
            view,
            disclosure: Disclosure::default(),
        }
    }
}
