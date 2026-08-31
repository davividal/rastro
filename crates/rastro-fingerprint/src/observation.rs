//! What was seen, and what the seer asserted about it.
//!
//! An observation knows nothing about documents, facets or JSON. It is a value
//! and the judgements a collector attached to it, because nobody downstream can
//! tell a self-changing value from a meaningful one, or a secret from a public
//! fact. Collectors classify; presentation decides what to do about it.

mod annotation;
mod scalar;

pub use annotation::{Sensitivity, Volatility};
pub use scalar::Scalar;

use std::collections::BTreeMap;

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

    /// This observation as `view` shows it, borrowed.
    ///
    /// Which values belong in which view is a rule about observations, so it
    /// lives here rather than in whatever happens to be rendering them. What a
    /// renderer gets is still an already-filtered tree it can encode without
    /// knowing that volatility exists — it just does not own it, which for a
    /// document of half a million walked paths is the difference between one
    /// copy and two.
    pub fn visible_in(&self, view: View) -> Option<Visible<'_>> {
        match view == View::Diffable && self.volatility == Volatility::Volatile {
            true => None,
            false => Some(Visible {
                observation: self,
                view,
            }),
        }
    }

    /// This observation as it appears in `view`, or nothing if the view drops
    /// it, as an owned tree.
    ///
    /// One rule, expressed once: this is [`Self::visible_in`] materialised. Kept
    /// for callers that want a document they own, chiefly tests asserting on the
    /// filtered shape; the renderer borrows instead.
    pub fn in_view(&self, view: View) -> Option<Self> {
        if view == View::Diffable && self.volatility == Volatility::Volatile {
            return None;
        }

        let content = match &self.content {
            Content::Scalar(scalar) => Content::Scalar(scalar.clone()),
            Content::Object(entries) => Content::Object(
                entries
                    .iter()
                    .filter_map(|(key, child)| {
                        child.in_view(view).map(|child| (key.clone(), child))
                    })
                    .collect(),
            ),
            Content::List(items) => {
                Content::List(items.iter().filter_map(|item| item.in_view(view)).collect())
            }
        };

        Some(Self {
            volatility: self.volatility,
            sensitivity: self.sensitivity,
            content,
        })
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

/// One observation as a view shows it, without copying the tree.
///
/// The filtering rule stays in this module; what travels to a renderer is this, which
/// applies the rule as it is walked. So a renderer still never asks whether anything is
/// volatile, and the document is not duplicated to answer the question for it.
#[derive(Debug, Clone, Copy)]
pub struct Visible<'a> {
    observation: &'a Observation,
    view: View,
}

/// What is inside a [`Visible`], with the values this view drops already gone.
#[derive(Debug, Clone, Copy)]
pub enum VisibleContent<'a> {
    Scalar(&'a Scalar),
    Object(VisibleObject<'a>),
    List(VisibleList<'a>),
}

#[derive(Debug, Clone, Copy)]
pub struct VisibleObject<'a> {
    entries: &'a BTreeMap<String, Observation>,
    view: View,
}

#[derive(Debug, Clone, Copy)]
pub struct VisibleList<'a> {
    items: &'a [Observation],
    view: View,
}

impl<'a> Visible<'a> {
    pub fn content(&self) -> VisibleContent<'a> {
        match &self.observation.content {
            Content::Scalar(scalar) => VisibleContent::Scalar(scalar),
            Content::Object(entries) => VisibleContent::Object(VisibleObject {
                entries,
                view: self.view,
            }),
            Content::List(items) => VisibleContent::List(VisibleList {
                items,
                view: self.view,
            }),
        }
    }
}

impl<'a> VisibleObject<'a> {
    /// The entries this view keeps, in the map's own sorted order.
    ///
    /// Sorted because the underlying map is, which is what makes an open shape's key order
    /// fixed without anybody choosing it.
    pub fn iter(&self) -> impl Iterator<Item = (&'a str, Visible<'a>)> + '_ {
        let view = self.view;

        self.entries.iter().filter_map(move |(key, child)| {
            child
                .visible_in(view)
                .map(|visible| (key.as_str(), visible))
        })
    }
}

impl<'a> VisibleList<'a> {
    pub fn iter(&self) -> impl Iterator<Item = Visible<'a>> + '_ {
        let view = self.view;

        self.items
            .iter()
            .filter_map(move |item| item.visible_in(view))
    }
}
