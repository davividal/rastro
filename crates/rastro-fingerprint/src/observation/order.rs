//! The order the port gives a set, over what a view shows of it.
//!
//! Structural and independent of any format: a set rendered as JSON and the same set
//! rendered any other way list their items alike, because nothing here reads bytes a
//! renderer produced. Integers compare by value, so `9` sorts before `10`.

use std::cmp::Ordering;

use super::{
    Completeness, Content, Observation, Scalar, Sensitivity, Visible, VisibleContent, Volatility,
};
use crate::presentation::Presentation;

/// Orders two items as a view shows them.
pub(super) fn compare(left: &Visible<'_>, right: &Visible<'_>) -> Ordering {
    match (left.content(), right.content()) {
        (VisibleContent::Scalar(left), VisibleContent::Scalar(right)) => {
            compare_scalars(&left, &right)
        }
        (VisibleContent::Object(left), VisibleContent::Object(right)) => {
            lexicographic(left.iter(), right.iter(), |left, right| {
                left.0.cmp(right.0).then_with(|| compare(&left.1, &right.1))
            })
        }
        (VisibleContent::Sequence(left), VisibleContent::Sequence(right)) => {
            lexicographic(left.iter(), right.iter(), compare)
        }
        (VisibleContent::Set(left), VisibleContent::Set(right)) => {
            lexicographic(left.iter(), right.iter(), compare)
        }
        (left, right) => shape_rank(&left).cmp(&shape_rank(&right)),
    }
}

/// Orders two items of a set by the fields its collector named, then as a whole.
///
/// A named field the view does not show sorts before one it does, so an item is never
/// ordered by a value the reader cannot see. The leading fields only apply to objects.
pub(super) fn compare_led_by(
    leading: &[String],
    left: &Visible<'_>,
    right: &Visible<'_>,
) -> Ordering {
    let by_leading = match (left.content(), right.content()) {
        (VisibleContent::Object(left), VisibleContent::Object(right)) => leading
            .iter()
            .map(|key| match (left.get(key), right.get(key)) {
                (None, None) => Ordering::Equal,
                (None, Some(_)) => Ordering::Less,
                (Some(_), None) => Ordering::Greater,
                (Some(left), Some(right)) => compare(&left, &right),
            })
            .find(|ordering| ordering.is_ne())
            .unwrap_or(Ordering::Equal),
        _ => Ordering::Equal,
    };

    by_leading.then_with(|| compare(left, right))
}

/// Sorts items as the complete, undisclosed view shows them, with the annotations
/// breaking a tie, so that two sets of the same items are equal however they were given.
pub(super) fn sort_observations(items: &mut [Observation], leading: &[String]) {
    let everything = Presentation::complete().raw();
    items.sort_by(|left, right| {
        let shown = match (left.visible_in(everything), right.visible_in(everything)) {
            (Some(left), Some(right)) => compare_led_by(leading, &left, &right),
            _ => unreachable!("the complete view drops nothing"),
        };
        shown.then_with(|| compare_annotations(left, right))
    });
}

/// Breaks a tie between two items whose content is equal by their annotations, root first
/// and then through the subtree, since a child marked volatile is a different item.
fn compare_annotations(left: &Observation, right: &Observation) -> Ordering {
    annotations_of(left)
        .cmp(&annotations_of(right))
        .then_with(|| match (&left.content, &right.content) {
            (Content::Object(left), Content::Object(right)) => {
                lexicographic(left.values(), right.values(), |left, right| {
                    compare_annotations(left, right)
                })
            }
            (Content::Sequence(left), Content::Sequence(right))
            | (Content::Set { items: left, .. }, Content::Set { items: right, .. }) => {
                lexicographic(left.iter(), right.iter(), |left, right| {
                    compare_annotations(left, right)
                })
            }
            _ => Ordering::Equal,
        })
}

fn annotations_of(observation: &Observation) -> (Volatility, Sensitivity, Completeness) {
    (
        observation.volatility,
        observation.sensitivity,
        observation.completeness,
    )
}

fn compare_scalars(left: &Scalar, right: &Scalar) -> Ordering {
    match (left, right) {
        (Scalar::Boolean(left), Scalar::Boolean(right)) => left.cmp(right),
        (Scalar::Integer(left), Scalar::Integer(right)) => left.cmp(right),
        (Scalar::Text(left), Scalar::Text(right)) => left.cmp(right),
        (left, right) => scalar_rank(left).cmp(&scalar_rank(right)),
    }
}

fn scalar_rank(scalar: &Scalar) -> u8 {
    match scalar {
        Scalar::Null => 0,
        Scalar::Boolean(_) => 1,
        Scalar::Integer(_) => 2,
        Scalar::Text(_) => 3,
    }
}

fn shape_rank(content: &VisibleContent<'_>) -> u8 {
    match content {
        VisibleContent::Scalar(_) => 0,
        VisibleContent::Object(_) => 1,
        VisibleContent::Sequence(_) => 2,
        VisibleContent::Set(_) => 3,
    }
}

/// Compares two runs of items pairwise, the shorter first where one is a prefix of the other.
fn lexicographic<T>(
    mut left: impl Iterator<Item = T>,
    mut right: impl Iterator<Item = T>,
    compare_items: impl Fn(&T, &T) -> Ordering,
) -> Ordering {
    loop {
        match (left.next(), right.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(left), Some(right)) => match compare_items(&left, &right) {
                Ordering::Equal => continue,
                unequal => return unequal,
            },
        }
    }
}
