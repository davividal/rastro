//! The wire shape of a fingerprint, and the only place that knows it.
//!
//! Encoding only. Which values belong in a view is a rule about observations
//! and lives in the domain, so this module receives an already-filtered *view*
//! of a tree and never asks whether anything is volatile. Borrowed rather than
//! copied: a filtered clone of a document with half a million walked paths costs
//! as much as the document.
//!
//! Determinism needs a *fixed* key order, and sorting is only one way to get
//! one, so the two kinds of object here are ordered differently:
//!
//! - **Fixed shapes** (document, facet, collector) are written by the `Wire*`
//!   types below, which emit entries in the order the statements appear. That
//!   order is chosen to read well: a facet leads with its `name`.
//! - **Open shapes** (whatever a collector observed) are written straight out
//!   of the domain's own `BTreeMap`, so their keys sort themselves. Sorting is
//!   the only fixed order available when the shape is not known in advance, and
//!   taking it from the model rather than from `serde_json::Map` is what retires
//!   the `preserve_order` hazard entirely: no map of serde_json's is involved.
//!
//! Output is pretty-printed rather than compact, because a fingerprint's
//! purpose is to be read by `diff`(1), and `diff` on a single-line document
//! tells you nothing.

use std::io;

use serde::ser::{SerializeMap, SerializeSeq};
use serde::{Serialize, Serializer};

use crate::collector::{CollectorCategory, CollectorIdentity};
use crate::facet::{Facet, FacetOutcome};
use crate::observation::{Scalar, Visible, VisibleContent};
use crate::presentation::Presentation;
use crate::{Fingerprint, SCHEMA_VERSION};

/// Renders a fingerprint as canonical JSON, with a trailing newline so the
/// document is a well-formed text file.
///
/// Prefer [`to_canonical_json_writer`] for anything but a small document: this
/// materialises the whole thing as one `String` first, and a fingerprint of a
/// real host is tens of megabytes.
pub fn to_canonical_json(
    fingerprint: &Fingerprint,
    presentation: impl Into<Presentation>,
) -> String {
    let mut rendered = Vec::new();
    to_canonical_json_writer(fingerprint, presentation, &mut rendered)
        .expect("a Vec accepts every byte");

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
    presentation: impl Into<Presentation>,
    out: &mut impl io::Write,
) -> io::Result<()> {
    let document = WireDocument {
        fingerprint,
        presentation: presentation.into(),
    };
    serde_json::to_writer_pretty(&mut *out, &document)?;

    out.write_all(b"\n")
}

/// The document: how to read it, who ran, then what was found.
struct WireDocument<'a> {
    fingerprint: &'a Fingerprint,
    presentation: Presentation,
}

/// One facet, leading with the name a reader scans for.
struct WireFacet<'a> {
    facet: &'a Facet,
    presentation: Presentation,
}

struct WireCollector<'a>(&'a CollectorIdentity);

impl<'a> WireDocument<'a> {
    fn facets_in(&self, category: CollectorCategory) -> Vec<WireFacet<'a>> {
        self.fingerprint
            .facets_in(category)
            .map(|facet| WireFacet {
                facet,
                presentation: self.presentation,
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
                if let Some(visible) = observation.visible_in(self.presentation) {
                    entries.serialize_entry("data", &WireObservation(visible))?;
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

/// Whatever a collector observed, written straight out of the tree it lives in.
///
/// The shape is open, so keys sort themselves: they come from the domain's own
/// `BTreeMap`, which is where the fixed order for an unknown shape comes from.
struct WireObservation<'a>(Visible<'a>);

impl Serialize for WireObservation<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self.0.content() {
            VisibleContent::Scalar(scalar) => match scalar.as_ref() {
                Scalar::Null => serializer.serialize_unit(),
                Scalar::Boolean(value) => serializer.serialize_bool(*value),
                Scalar::Integer(value) => serializer.serialize_i64(*value),
                Scalar::Text(value) => serializer.serialize_str(value),
            },
            VisibleContent::Object(entries) => {
                let mut object = serializer.serialize_map(None)?;
                for (key, child) in entries.iter() {
                    object.serialize_entry(key, &WireObservation(child))?;
                }
                object.end()
            }
            VisibleContent::List(items) => {
                let mut list = serializer.serialize_seq(None)?;
                for item in items.iter() {
                    list.serialize_element(&WireObservation(item))?;
                }
                list.end()
            }
        }
    }
}
