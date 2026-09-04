//! The web server this box would serve with.

use rastro_collector::Observation;

use crate::collectors::nginx::model::{Binary, Configuration, HttpService, Master, StreamService};

/// Everything the facet reports: the binary, the configuration it would read, the master
/// running it, and the two services that configuration describes.
///
/// The binary and the configuration are kept apart on purpose. A package upgrade changes the
/// binary and leaves the configuration alone; an edit does the opposite; and a reader looking
/// at a diff needs to see which of the two moved.
///
/// `http` and `stream` are a projection of the same configuration, not a second reading of
/// it: they name what the model understands, while `configuration.files` digests everything
/// each file says, modelled or not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebServer {
    pub binary: Binary,
    /// The running master, or nothing when nginx is installed and stopped.
    pub master: Option<Master>,
    pub configuration: Configuration,
    pub http: HttpService,
    pub stream: StreamService,
}

impl From<&WebServer> for Observation {
    fn from(server: &WebServer) -> Self {
        Observation::object([
            ("binary", Observation::from(&server.binary)),
            ("configuration", Observation::from(&server.configuration)),
            ("http", Observation::from(&server.http)),
            (
                "master",
                server
                    .master
                    .as_ref()
                    .map_or_else(Observation::null, Observation::from),
            ),
            ("stream", Observation::from(&server.stream)),
        ])
    }
}
