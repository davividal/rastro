//! The wire shape of a fingerprint, and the only place that knows it.
//!
//! Encoding only. Which values belong in a view is a rule about observations
//! and lives in the domain, so this module receives an already-filtered tree
//! and never asks whether anything is volatile.
//!
//! Determinism needs a *fixed* key order, and sorting is only one way to get
//! one, so the two kinds of object here are ordered differently:
//!
//! - **Fixed shapes** (document, facet, collector) are written by the `Wire*`
//!   types below, which emit entries in the order the statements appear. That
//!   order is chosen to read well: a facet leads with its `name`.
//! - **Open shapes** (whatever a collector observed) go through
//!   [`serde_json::Value`], whose map is a `BTreeMap`, so their keys sort
//!   themselves. Sorting is the only fixed order available when the shape is
//!   not known in advance. Enabling serde_json's `preserve_order` feature would
//!   silently break that.
//!
//! Output is pretty-printed rather than compact, because a fingerprint's
//! purpose is to be read by `diff`(1), and `diff` on a single-line document
//! tells you nothing.

use std::io;

use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};
use serde_json::{Map, Value};

use crate::collector::{CollectorCategory, CollectorIdentity};
use crate::facet::{Facet, FacetOutcome};
use crate::observation::{Content, Observation, Scalar};
use crate::view::View;
use crate::{Fingerprint, SCHEMA_VERSION};

/// Renders a fingerprint as canonical JSON, with a trailing newline so the
/// document is a well-formed text file.
///
/// Prefer [`to_canonical_json_writer`] for anything but a small document: this
/// materialises the whole thing as one `String` first, and a fingerprint of a
/// real host is tens of megabytes.
pub fn to_canonical_json(fingerprint: &Fingerprint, view: View) -> String {
    let mut rendered = Vec::new();
    to_canonical_json_writer(fingerprint, view, &mut rendered).expect("a Vec accepts every byte");

    String::from_utf8(rendered).expect("a fingerprint document is always UTF-8")
}

/// Writes a fingerprint as canonical JSON, straight into `out`.
///
/// The document is never assembled in memory as a whole: a host with half a
/// million walked paths would otherwise pay for a second copy of itself purely
/// to be written out.
///
/// **Only the io failure is reachable.** `serde_json` can fail either because a
/// value will not serialise or because the writer refused, and the first cannot
/// happen here: every leaf is `null`, a bool, an `i64` or a `String`, and the
/// format admits no float or non-string key. So a failure out of this is the
/// disk, and the caller names the path, since this module has no business
/// knowing where the bytes were going.
pub fn to_canonical_json_writer(
    fingerprint: &Fingerprint,
    view: View,
    out: &mut impl io::Write,
) -> io::Result<()> {
    let document = WireDocument { fingerprint, view };
    serde_json::to_writer_pretty(&mut *out, &document)?;

    out.write_all(b"\n")
}

/// The document: how to read it, who ran, then what was found.
struct WireDocument<'a> {
    fingerprint: &'a Fingerprint,
    view: View,
}

/// One facet, leading with the name a reader scans for.
struct WireFacet<'a> {
    facet: &'a Facet,
    view: View,
}

struct WireCollector<'a>(&'a CollectorIdentity);

impl<'a> WireDocument<'a> {
    fn facets_in(&self, category: CollectorCategory) -> Vec<WireFacet<'a>> {
        self.fingerprint
            .facets_in(category)
            .map(|facet| WireFacet {
                facet,
                view: self.view,
            })
            .collect()
    }
}

impl Serialize for WireDocument<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let metadata = self.facets_in(CollectorCategory::Metadata);
        let facets = self.facets_in(CollectorCategory::State);

        let mut document = serializer.serialize_map(Some(3))?;
        document.serialize_entry("schema_version", &SCHEMA_VERSION)?;
        document.serialize_entry("metadata", &metadata)?;
        document.serialize_entry("facets", &facets)?;
        document.end()
    }
}

impl Serialize for WireFacet<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut entries = serializer.serialize_map(None)?;
        entries.serialize_entry("name", self.facet.name.as_str())?;
        entries.serialize_entry("collector", &WireCollector(&self.facet.collector))?;

        match &self.facet.outcome {
            FacetOutcome::Ok { observation } => {
                entries.serialize_entry("status", "ok")?;
                if let Some(visible) = observation.in_view(self.view) {
                    entries.serialize_entry("data", &observation_value(&visible))?;
                }
            }
            FacetOutcome::Absent => {
                entries.serialize_entry("status", "absent")?;
            }
            FacetOutcome::Error { message } => {
                entries.serialize_entry("status", "error")?;
                entries.serialize_entry("error", message)?;
            }
        }

        entries.end()
    }
}

impl Serialize for WireCollector<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut entries = serializer.serialize_map(Some(2))?;
        entries.serialize_entry("id", self.0.id.as_str())?;
        entries.serialize_entry("version", self.0.version.as_str())?;
        entries.end()
    }
}

/// Whatever a collector observed, as JSON.
///
/// The shape is open, so keys sort themselves through the `BTreeMap` behind
/// [`serde_json::Value`].
fn observation_value(observation: &Observation) -> Value {
    match observation.content() {
        Content::Scalar(scalar) => scalar_value(scalar),
        Content::Object(entries) => Value::Object(
            entries
                .iter()
                .map(|(key, child)| (key.clone(), observation_value(child)))
                .collect::<Map<String, Value>>(),
        ),
        Content::List(items) => Value::Array(items.iter().map(observation_value).collect()),
    }
}

fn scalar_value(scalar: &Scalar) -> Value {
    match scalar {
        Scalar::Null => Value::Null,
        Scalar::Boolean(value) => Value::Bool(*value),
        Scalar::Integer(value) => Value::from(*value),
        Scalar::Text(value) => Value::String(value.clone()),
    }
}
