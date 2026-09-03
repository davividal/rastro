//! What was seen, and what the seer asserted about it.
//!
//! An observation knows nothing about documents, facets or JSON. It is a value
//! and the judgements a collector attached to it, because nobody downstream can
//! tell a self-changing value from a meaningful one, or a secret from a public
//! fact. Collectors classify; presentation decides what to do about it.

mod annotation;
pub mod redaction;
mod scalar;

pub use annotation::{Sensitivity, Volatility};
pub use scalar::Scalar;

use std::borrow::Cow;
use std::collections::BTreeMap;

use crate::digest::Xxh3Digest;
use crate::presentation::{Disclosure, Presentation};
use crate::view::View;

/// One observed value, with everything the collector knew about it.
///
/// Annotations sit on every node, not only on leaves, so a collector marks a
/// whole subtree volatile in one move rather than tagging each value under it.
#[derive(Debug, Clone, PartialEq)]
pub struct Observation {
    volatility: Volatility,
    sensitivity: Sensitivity,
    content: Content,
}

/// The shape of an observed value.
///
/// Object keys are unconstrained strings because collectors legitimately key by
/// file paths and unit names. They live in a [`BTreeMap`] so that ordering is a
/// property of the structure rather than of collector discipline.
#[derive(Debug, Clone, PartialEq)]
pub enum Content {
    Scalar(Scalar),
    Object(BTreeMap<String, Observation>),
    List(Vec<Observation>),
}

impl Observation {
    pub fn null() -> Self {
        Self::scalar(Scalar::Null)
    }

    pub fn boolean(value: bool) -> Self {
        Self::scalar(Scalar::Boolean(value))
    }

    pub fn integer(value: i64) -> Self {
        Self::scalar(Scalar::Integer(value))
    }

    pub fn text(value: impl Into<String>) -> Self {
        Self::scalar(Scalar::Text(value.into()))
    }

    pub fn scalar(value: Scalar) -> Self {
        Self::unannotated(Content::Scalar(value))
    }

    pub fn object<K: Into<String>>(entries: impl IntoIterator<Item = (K, Observation)>) -> Self {
        let entries = entries
            .into_iter()
            .map(|(key, observation)| (key.into(), observation))
            .collect();
        Self::unannotated(Content::Object(entries))
    }

    pub fn list(items: impl IntoIterator<Item = Observation>) -> Self {
        Self::unannotated(Content::List(items.into_iter().collect()))
    }

    /// Marks this value, and everything under it, as self-changing.
    pub fn volatile(mut self) -> Self {
        self.volatility = Volatility::Volatile;
        self
    }

    /// Marks this value, and everything under it, as needing redaction.
    pub fn sensitive(mut self) -> Self {
        self.sensitivity = Sensitivity::Sensitive;
        self
    }

    /// This observation as `presentation` shows it, borrowed.
    ///
    /// Which values belong in which view, and which stand in as digests, are
    /// rules about observations, so they live here rather than in whatever
    /// happens to be rendering them. What a renderer gets is still an
    /// already-filtered tree it can encode without knowing that volatility or
    /// sensitivity exist — it just does not own it, which for a document of half
    /// a million walked paths is the difference between one copy and two.
    ///
    /// Takes anything that converts, so the 70-odd callers that care only about
    /// the view still pass a [`View`] and get the safe disclosure.
    pub fn visible_in(&self, presentation: impl Into<Presentation>) -> Option<Visible<'_>> {
        self.visible_under(presentation.into(), false)
    }

    /// This observation as it appears in `presentation`, or nothing if the view
    /// drops it, as an owned tree.
    ///
    /// One rule, expressed once: this is [`Self::visible_in`] materialised. Kept
    /// for callers that want a document they own, chiefly tests asserting on the
    /// filtered shape; the renderer borrows instead.
    pub fn in_view(&self, presentation: impl Into<Presentation>) -> Option<Self> {
        self.materialised(presentation.into(), false)
    }

    /// The borrowed view, with `inherited` saying an ancestor was withheld.
    ///
    /// Sensitivity descends: annotating a node covers everything under it, so a child of a
    /// withheld object is withheld whatever its own annotation says.
    fn visible_under(&self, presentation: Presentation, inherited: bool) -> Option<Visible<'_>> {
        match self.dropped_by(presentation) {
            true => None,
            false => Some(Visible {
                observation: self,
                presentation,
                withheld: self.withheld_under(presentation, inherited),
            }),
        }
    }

    fn materialised(&self, presentation: Presentation, inherited: bool) -> Option<Self> {
        if self.dropped_by(presentation) {
            return None;
        }

        let withheld = self.withheld_under(presentation, inherited);
        let content = match &self.content {
            Content::Scalar(scalar) => Content::Scalar(stood_in_for(scalar, withheld)),
            Content::Object(entries) => Content::Object(
                entries
                    .iter()
                    .filter_map(|(key, child)| {
                        child
                            .materialised(presentation, withheld)
                            .map(|child| (key.clone(), child))
                    })
                    .collect(),
            ),
            Content::List(items) => Content::List(
                items
                    .iter()
                    .filter_map(|item| item.materialised(presentation, withheld))
                    .collect(),
            ),
        };

        Some(Self {
            volatility: self.volatility,
            sensitivity: self.sensitivity,
            content,
        })
    }

    /// Whether this view omits the value altogether, which only volatility does.
    fn dropped_by(&self, presentation: Presentation) -> bool {
        presentation.view() == View::Diffable && self.volatility == Volatility::Volatile
    }

    /// Whether this value stands in as a digest rather than appearing as it is.
    ///
    /// The annotation survives either way: what the collector judged does not change with
    /// how the document is being rendered, which is what lets a reader of a `--raw`
    /// document still see which values were the sensitive ones.
    fn withheld_under(&self, presentation: Presentation, inherited: bool) -> bool {
        presentation.disclosure() == Disclosure::Redacted
            && (inherited || self.sensitivity == Sensitivity::Sensitive)
    }

    pub fn volatility(&self) -> Volatility {
        self.volatility
    }

    pub fn sensitivity(&self) -> Sensitivity {
        self.sensitivity
    }

    pub fn content(&self) -> &Content {
        &self.content
    }

    fn unannotated(content: Content) -> Self {
        Self {
            volatility: Volatility::default(),
            sensitivity: Sensitivity::default(),
            content,
        }
    }
}

/// Here rather than beside the digest, because the dependency runs this way: redaction
/// reaches for a digest, so a digest that reached back for an observation would make the two
/// modules one pretending to be two. `tests/purity.rs` holds that line.
impl From<&Xxh3Digest> for Observation {
    fn from(digest: &Xxh3Digest) -> Self {
        Observation::text(digest.as_str())
    }
}

/// The scalar a presentation shows, which is the value itself or a digest standing in.
///
/// Owned only where something was withheld: a document of half a million walked paths
/// borrows every one of its scalars, and the handful of secrets on a box are the only
/// allocations redaction costs.
fn stood_in_for(scalar: &Scalar, withheld: bool) -> Scalar {
    match withheld.then(|| redaction::redacted(scalar)).flatten() {
        Some(stand_in) => Scalar::Text(stand_in),
        None => scalar.clone(),
    }
}

/// One observation as a presentation shows it, without copying the tree.
///
/// The filtering rule stays in this module; what travels to a renderer is this, which
/// applies the rule as it is walked. So a renderer still never asks whether anything is
/// volatile or sensitive, and the document is not duplicated to answer the question for it.
#[derive(Debug, Clone, Copy)]
pub struct Visible<'a> {
    observation: &'a Observation,
    presentation: Presentation,
    /// Whether this node's value stands in as a digest, this node's own annotation or an
    /// ancestor's. Resolved on the way down, because a child cannot see its ancestors.
    withheld: bool,
}

/// What is inside a [`Visible`], with the values this view drops already gone and the ones
/// it withholds already replaced.
#[derive(Debug, Clone)]
pub enum VisibleContent<'a> {
    /// Borrowed where the value is shown, owned where a digest stands in for it.
    Scalar(Cow<'a, Scalar>),
    Object(VisibleObject<'a>),
    List(VisibleList<'a>),
}

#[derive(Debug, Clone, Copy)]
pub struct VisibleObject<'a> {
    entries: &'a BTreeMap<String, Observation>,
    presentation: Presentation,
    withheld: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct VisibleList<'a> {
    items: &'a [Observation],
    presentation: Presentation,
    withheld: bool,
}

impl<'a> Visible<'a> {
    pub fn content(&self) -> VisibleContent<'a> {
        match &self.observation.content {
            Content::Scalar(scalar) => VisibleContent::Scalar(
                match self.withheld.then(|| redaction::redacted(scalar)).flatten() {
                    Some(stand_in) => Cow::Owned(Scalar::Text(stand_in)),
                    None => Cow::Borrowed(scalar),
                },
            ),
            Content::Object(entries) => VisibleContent::Object(VisibleObject {
                entries,
                presentation: self.presentation,
                withheld: self.withheld,
            }),
            Content::List(items) => VisibleContent::List(VisibleList {
                items,
                presentation: self.presentation,
                withheld: self.withheld,
            }),
        }
    }

    /// What the collector judged, which a presentation never changes.
    pub fn sensitivity(&self) -> Sensitivity {
        self.observation.sensitivity
    }
}

impl<'a> VisibleObject<'a> {
    /// The entries this view keeps, in the map's own sorted order.
    ///
    /// Sorted because the underlying map is, which is what makes an open shape's key order
    /// fixed without anybody choosing it.
    pub fn iter(&self) -> impl Iterator<Item = (&'a str, Visible<'a>)> + '_ {
        let presentation = self.presentation;
        let inherited = self.withheld;

        self.entries.iter().filter_map(move |(key, child)| {
            child
                .visible_under(presentation, inherited)
                .map(|visible| (key.as_str(), visible))
        })
    }
}

impl<'a> VisibleList<'a> {
    pub fn iter(&self) -> impl Iterator<Item = Visible<'a>> + '_ {
        let presentation = self.presentation;
        let inherited = self.withheld;

        self.items
            .iter()
            .filter_map(move |item| item.visible_under(presentation, inherited))
    }
}
