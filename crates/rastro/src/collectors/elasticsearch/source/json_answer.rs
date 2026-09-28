//! Reading a node's JSON answer, with the field that failed named.

use serde::de::DeserializeOwned;

use crate::collectors::elasticsearch::value_objects::Unread;

/// `body`, the answer to `GET path`, read into `T`.
///
/// **Which field, not which byte**, for the reason the RabbitMQ reader gives: the operator who
/// reads the error no longer has the answer, so `at line 1 column 4889` names nothing.
pub fn read_answer<T: DeserializeOwned>(body: &str, path: &str) -> Result<T, Unread> {
    let mut deserializer = serde_json::Deserializer::from_str(body);
    let answer: T = serde_path_to_error::deserialize(&mut deserializer).map_err(|failure| {
        Unread::new(format!(
            "the answer to GET {path} could not be read at {}: {}",
            failure.path(),
            failure.inner()
        ))
    })?;
    deserializer.end().map_err(|trailing| {
        Unread::new(format!(
            "the answer to GET {path} has more after it: {trailing}"
        ))
    })?;

    Ok(answer)
}
