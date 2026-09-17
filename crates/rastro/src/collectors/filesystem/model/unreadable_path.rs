//! A path the walk reached and did not describe.

use rastro_collector::{AbsolutePath, NonEmptyText, Observation};

use super::Refusal;

/// A path that is there, and the reason it is not in the document as itself.
///
/// Two reasons reach here, and they render the same because a reader wants the same thing
/// from both: the host refused rastro, or rastro declined to walk a tree it could not
/// account for. What the type holds is a path recorded as a reason rather than as
/// attributes, whichever of the two put it there.
///
/// **An entry is its attributes or the reason it has none, never a partial set pretending to
/// be complete.** That is the facet contract one level down: a facet carries `data` or
/// `error`, and so does a path. Recording half an entry beside the failure would leave a
/// reader unable to tell which of the attributes were observed and which were left at
/// whatever the code reached before it gave up.
///
/// **Not volatile.** A path rastro cannot read is a lasting blind spot in the fingerprint,
/// and the default view is the one an operator diffs, so hiding it there would be hiding the
/// gap rather than reporting it. The reasons that reach here are persistent by construction
/// — the ones that are not are omitted instead, see
/// [`is_absence`](super::is_absence).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnreadablePath {
    pub path: AbsolutePath,
    pub reason: NonEmptyText,
}

impl UnreadablePath {
    /// What the document records about a refused path, or nothing when the path simply went
    /// away.
    ///
    /// Returns rather than appending to a list the caller owns, so that both answers are one
    /// value a test can state, and the walk keeps its accumulator to itself.
    pub fn recorded(path: &AbsolutePath, refusal: Refusal) -> Option<Self> {
        let Refusal::Unreadable(reason) = refusal else {
            return None;
        };

        Some(Self {
            path: path.clone(),
            reason: NonEmptyText::new(reason, "a refusal")
                .expect("a refusal carries at least a path and a reason"),
        })
    }

    /// A tree more than one claim named, which the walk sealed rather than entering.
    ///
    /// The claimants are the whole reason: a tree with nothing under it is an ordinary seal
    /// until a reader knows that claims argued over it, and which ones. The sentence is the
    /// rule's own, so the operator's warning and this entry cannot drift apart.
    pub fn contested(path: &AbsolutePath, contest: String) -> Self {
        Self {
            path: path.clone(),
            reason: NonEmptyText::new(contest, "a refusal")
                .expect("a contest names at least two claimants"),
        }
    }
}

impl From<&UnreadablePath> for Observation {
    fn from(refused: &UnreadablePath) -> Self {
        Observation::object([("error", Observation::text(refused.reason.as_str()))])
    }
}
