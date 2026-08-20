//! What this box is localised to.
//!
//! Three layers, and the dependency arrows only point one way:
//! [`source`] knows [`model`], `model` knows [`value_objects`], and neither of the last
//! two knows a host interface exists.
//!
//! **Small, and it changes the output of everything.** A locale decides how every program on
//! the box sorts, formats a number and renders a date, which is why rastro's own execution
//! seam forces `LC_ALL=C` on every tool it runs: without it this facet's own value would
//! change the bytes of other facets. That the tool pins the locale and also records it is
//! deliberate, not circular — the pinning is about rastro's reading, the record is about the
//! host.
//!
//! Kept apart from the `time` facet, which the neighbouring `systemd` tool answers for, so
//! that neither facet is named after half of what it holds.
pub mod model;
pub mod source;
pub mod value_objects;

pub use model::Localisation;
pub use source::LocaleFiles;
pub use value_objects::{SettingName, SettingValue};

// One import, because `rastro-collector` re-exports what an author needs. A
// collector written outside this repo looks exactly like this.
use rastro_collector::{
    CollectionError, Collector, CollectorCategory, CollectorId, CollectorIdentity,
    CollectorVersion, FacetName, Observation, Presence,
};

pub struct LocaleCollector {
    name: FacetName,
    identity: CollectorIdentity,
    files: LocaleFiles,
}

impl LocaleCollector {
    pub fn new() -> Self {
        Self::reading(LocaleFiles::new())
    }

    /// The same collector over a source the caller chose.
    pub fn reading(files: LocaleFiles) -> Self {
        Self {
            name: FacetName::new("locale").expect("`locale` is a legal facet name"),
            identity: CollectorIdentity::new(
                CollectorId::new("locale").expect("`locale` is a legal collector id"),
                CollectorVersion::new("1").expect("`1` is a legal collector version"),
            ),
            files,
        }
    }
}

impl Default for LocaleCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl Collector for LocaleCollector {
    fn name(&self) -> &FacetName {
        &self.name
    }

    fn identity(&self) -> &CollectorIdentity {
        &self.identity
    }

    fn category(&self) -> CollectorCategory {
        CollectorCategory::State
    }

    /// Always present, because the subject is the files rastro reads and it can always report
    /// on those.
    ///
    /// A box where none of them exists is not absent of localisation — it falls back to the C
    /// locale — so the honest answer is a facet saying every file is absent, which is exactly
    /// what the data does say.
    fn presence(&self) -> Presence {
        Presence::Present
    }

    fn collect(&self) -> Result<Observation, CollectionError> {
        Ok(Observation::from(&self.files.read()?))
    }
}
