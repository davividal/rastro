//! One read of a node's API, which fails on its own.

use rastro_collector::Observation;

use crate::collectors::elasticsearch::value_objects::Unread;

/// One of a node's surfaces as read: its answer, or why there was none.
///
/// **A surface fails alone.** A node without ILM, as an OSS build of 7.x is, still has
/// templates and settings worth reporting, so one refusal marks that surface and the rest are
/// read. A node that could not be asked at all fails on the node instead, before any surface.
pub type Surface<Answer> = Result<Answer, Unread>;

/// A surface rendered: its own observation, or its refusal marked as incomplete.
pub fn surface_observation<Answer>(
    surface: Option<&Surface<Answer>>,
    render: impl FnOnce(&Answer) -> Observation,
) -> Observation {
    match surface {
        None => Observation::null(),
        Some(Ok(answer)) => render(answer),
        Some(Err(unread)) => {
            Observation::object([("error", Observation::text(unread.reason()))]).incomplete()
        }
    }
}
