//! Where a podman service says it keeps things.

use rastro_collector::{AbsolutePath, Observation};

use crate::collectors::containers::value_objects::StorageDriver;

/// The store as the *service* resolved it.
///
/// **Kept beside what the configuration files say, so the two can disagree.** rastro reads
/// podman's configuration itself to build the filesystem claim, because a box with no
/// service running cannot be asked anything; where a service does answer, this is its own
/// account of the same question. A store moved in a file that podman has not been restarted
/// to pick up is exactly the kind of disagreement worth seeing, and it is invisible in
/// either half alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PodmanStore {
    pub graph_root: Option<AbsolutePath>,
    pub run_root: Option<AbsolutePath>,
    pub volume_path: Option<AbsolutePath>,
    pub driver: Option<StorageDriver>,
}

impl From<&PodmanStore> for Observation {
    fn from(store: &PodmanStore) -> Self {
        Observation::object([
            ("driver", optional_driver(store.driver.as_ref())),
            ("graph_root", path(store.graph_root.as_ref())),
            ("run_root", path(store.run_root.as_ref())),
            ("volume_path", path(store.volume_path.as_ref())),
        ])
    }
}

fn path(value: Option<&AbsolutePath>) -> Observation {
    match value {
        Some(value) => Observation::text(value.as_str()),
        None => Observation::null(),
    }
}

fn optional_driver(driver: Option<&StorageDriver>) -> Observation {
    match driver {
        Some(driver) => Observation::from(driver),
        None => Observation::null(),
    }
}
