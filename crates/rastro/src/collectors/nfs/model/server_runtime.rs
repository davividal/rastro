//! How the kernel server is running.

use std::collections::BTreeMap;

use rastro_collector::Observation;

use crate::collectors::nfs::value_objects::KernelValue;

/// The server's runtime settings, keyed by the name nfsd gives each.
///
/// What the running server does, which `/etc/nfs.conf` only asks for: `rpc.nfsd` applies the
/// file at start, and anything since, `rpc.nfsd 16` by hand included, is only here.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ServerRuntime(BTreeMap<String, KernelValue>);

impl ServerRuntime {
    pub fn new(settings: impl IntoIterator<Item = (String, KernelValue)>) -> Self {
        Self(settings.into_iter().collect())
    }

    pub fn settings(&self) -> &BTreeMap<String, KernelValue> {
        &self.0
    }
}

impl From<&ServerRuntime> for Observation {
    fn from(runtime: &ServerRuntime) -> Self {
        Observation::object(
            runtime
                .settings()
                .iter()
                .map(|(name, value)| (name.as_str(), Observation::from(value))),
        )
    }
}
