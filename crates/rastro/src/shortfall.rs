//! What a run could not see, said to the operator rather than only written down.
//!
//! **The document already records every failure, and that was not enough.** A facet that
//! failed says so in its status, and an item a collector was refused carries its reason, but
//! both sit in a file the operator has not opened yet. An unprivileged run finished with a
//! progress line that came and went and a document missing three facets and a broker, and
//! nothing on the terminal said so. This is the summary of what is missing, after the
//! document is safely written, so a partial run never passes for a whole one.
//!
//! Two kinds, because a status alone misses the second: a facet that failed outright, named
//! with its reason; and an `ok` facet holding items its collector marked
//! [`Completeness::Incomplete`](rastro_fingerprint::Completeness), counted. Counted rather
//! than listed, because a walk refused under someone else's home directory is refused
//! thousands of times, and the document holds every one of them.

use rastro_fingerprint::{FacetName, FacetOutcome, Fingerprint};

/// Everything one run could not see.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shortfall {
    failed: Vec<FailedFacet>,
    incomplete: Vec<IncompleteFacet>,
}

/// A facet whose collector failed, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailedFacet {
    pub name: FacetName,
    pub reason: String,
}

/// A facet that was read, holding items its collector could not read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncompleteFacet {
    pub name: FacetName,
    pub items: usize,
}

impl Shortfall {
    /// What `fingerprint` is missing, in the document's own facet order.
    pub fn of(fingerprint: &Fingerprint) -> Self {
        let mut failed = Vec::new();
        let mut incomplete = Vec::new();

        for facet in fingerprint.facets() {
            match &facet.outcome {
                FacetOutcome::Error { message } => failed.push(FailedFacet {
                    name: facet.name.clone(),
                    reason: message.clone(),
                }),
                FacetOutcome::Ok { observation } => {
                    let items = observation.incomplete_items();
                    if items > 0 {
                        incomplete.push(IncompleteFacet {
                            name: facet.name.clone(),
                            items,
                        });
                    }
                }
                FacetOutcome::Absent => {}
            }
        }

        failed.sort_by(|left, right| left.name.cmp(&right.name));
        incomplete.sort_by(|left, right| left.name.cmp(&right.name));

        Self { failed, incomplete }
    }

    pub fn is_empty(&self) -> bool {
        self.failed.is_empty() && self.incomplete.is_empty()
    }

    /// One message per kind of loss, failed facets first since they are the larger one.
    pub fn messages(&self) -> Vec<String> {
        let mut messages = Vec::new();

        if !self.failed.is_empty() {
            let lines = self.failed.iter().map(|facet| {
                format!(
                    "\n  {}: {}",
                    facet.name.as_str(),
                    first_line_of(&facet.reason)
                )
            });
            messages.push(format!(
                "{} could not be read:{}",
                counted(self.failed.len(), "facet", "facets"),
                lines.collect::<String>()
            ));
        }

        if !self.incomplete.is_empty() {
            let lines = self.incomplete.iter().map(|facet| {
                format!(
                    "\n  {}: {} could not be read",
                    facet.name.as_str(),
                    counted(facet.items, "item", "items")
                )
            });
            messages.push(format!(
                "{} incomplete:{}",
                match self.incomplete.len() {
                    1 => "1 facet is".to_owned(),
                    many => format!("{many} facets are"),
                },
                lines.collect::<String>()
            ));
        }

        messages
    }
}

/// A reason as one inert line, marked where it was cut.
///
/// A tool's usage text can be the whole of a reason, and on stderr it would break the list of
/// one facet per line. The document keeps every line, so nothing is lost by cutting here.
///
/// **Control characters are written out, not passed through.** A reason carries a tool's
/// stderr or a path on the host, and either can hold an escape sequence: printed raw, it can
/// retitle the terminal or, with a bare carriage return, overwrite the warning it is part of.
fn first_line_of(reason: &str) -> String {
    let mut lines = reason.trim_end().lines();
    let first: String = lines
        .next()
        .unwrap_or_default()
        .chars()
        .map(|character| match character.is_control() {
            true => character.escape_default().to_string(),
            false => character.to_string(),
        })
        .collect();
    match lines.next() {
        Some(_) => format!("{first} […]"),
        None => first,
    }
}

fn counted(count: usize, one: &str, many: &str) -> String {
    match count {
        1 => format!("1 {one}"),
        _ => format!("{count} {many}"),
    }
}
