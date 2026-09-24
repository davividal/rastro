//! One virtual host on a node.

use rastro_collector::Observation;

/// A virtual host as the definitions export describes it.
///
/// The unit of tenancy on a broker: every queue, exchange, binding and permission belongs to
/// one, so a vhost appearing or disappearing is the largest single change this facet can
/// report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Vhost {
    /// What a queue declared without a type becomes, which is a policy decision made once at
    /// creation and invisible in any queue's own definition.
    pub default_queue_type: Option<String>,

    pub description: Option<String>,
    pub tags: Vec<String>,
}

impl From<&Vhost> for Observation {
    fn from(vhost: &Vhost) -> Self {
        Observation::object([
            (
                "default_queue_type",
                match &vhost.default_queue_type {
                    Some(queue_type) => Observation::text(queue_type.as_str()),
                    None => Observation::null(),
                },
            ),
            (
                "description",
                match &vhost.description {
                    Some(description) => Observation::text(description.as_str()),
                    None => Observation::null(),
                },
            ),
            (
                "tags",
                Observation::list(vhost.tags.iter().map(|tag| Observation::text(tag.as_str()))),
            ),
        ])
    }
}
