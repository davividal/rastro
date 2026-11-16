//! Everything the server exports.

use std::collections::BTreeMap;

use rastro_collector::{AbsolutePath, CollectionError, Observation};

use super::client_export::ClientExport;
use crate::collectors::nfs::value_objects::ExportClient;

/// The exports, keyed by path and then by client.
///
/// Keyed both ways because `exportfs` keeps one entry per pair, and adding a client to a path
/// then reads as one added key rather than a list that moved.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExportTable(BTreeMap<AbsolutePath, BTreeMap<ExportClient, ClientExport>>);

impl ExportTable {
    /// Files each export under its path and client.
    ///
    /// A repeated pair is refused rather than overwritten. `exportfs` merges them before it
    /// writes, so one means the table was misread, and keeping either would drop an export
    /// from a table claiming to be complete.
    pub fn new(
        exports: impl IntoIterator<Item = (AbsolutePath, ExportClient, ClientExport)>,
    ) -> Result<Self, CollectionError> {
        let mut table: BTreeMap<AbsolutePath, BTreeMap<ExportClient, ClientExport>> =
            BTreeMap::new();

        for (path, client, export) in exports {
            let clients = table.entry(path.clone()).or_default();
            if clients.contains_key(&client) {
                return Err(CollectionError::new(format!(
                    "{:?} was exported to {:?} twice, so the table was misread",
                    path.as_str(),
                    client.as_str()
                )));
            }
            clients.insert(client, export);
        }

        Ok(Self(table))
    }

    pub fn exports(&self) -> &BTreeMap<AbsolutePath, BTreeMap<ExportClient, ClientExport>> {
        &self.0
    }
}

impl From<&ExportTable> for Observation {
    fn from(table: &ExportTable) -> Self {
        Observation::object(table.exports().iter().map(|(path, clients)| {
            (
                path.as_str(),
                Observation::object(
                    clients
                        .iter()
                        .map(|(client, export)| (client.as_str(), Observation::from(export))),
                ),
            )
        }))
    }
}
