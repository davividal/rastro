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

    /// This observation as it appears in `view`, or nothing if the view drops
    /// it.
    ///
    /// Which values belong in which view is a rule about observations, so it
    /// lives here rather than in whatever happens to be rendering them. The
    /// result is a filtered tree that a renderer can encode without knowing
    /// that volatility exists.
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
