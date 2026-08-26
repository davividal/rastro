#![allow(dead_code)]

use rastro_fingerprint::{Content, Observation, Scalar};

pub fn object_of(observation: &Observation) -> Vec<(String, Observation)> {
    match observation.content() {
        Content::Object(entries) => entries
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
        other => panic!("expected an object, got {other:?}"),
    }
}

pub fn field(observation: &Observation, name: &str) -> Observation {
    object_of(observation)
        .into_iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value)
        .unwrap_or_else(|| panic!("expected a {name:?} field"))
}

pub fn items_of(observation: &Observation) -> Vec<Observation> {
    match observation.content() {
        Content::List(items) => items.clone(),
        other => panic!("expected a list, got {other:?}"),
    }
}

pub fn keys_of(observation: &Observation) -> Vec<String> {
    object_of(observation)
        .into_iter()
        .map(|(key, _)| key)
        .collect()
}

pub fn text(observation: &Observation) -> String {
    match observation.content() {
        Content::Scalar(Scalar::Text(value)) => value.clone(),
        other => panic!("expected text, got {other:?}"),
    }
}

pub fn integer(observation: &Observation) -> i64 {
    match observation.content() {
        Content::Scalar(Scalar::Integer(value)) => *value,
        other => panic!("expected an integer, got {other:?}"),
    }
}

pub fn boolean(observation: &Observation) -> bool {
    match observation.content() {
        Content::Scalar(Scalar::Boolean(value)) => *value,
        other => panic!("expected a boolean, got {other:?}"),
    }
}

pub fn is_null(observation: &Observation) -> bool {
    matches!(observation.content(), Content::Scalar(Scalar::Null))
}
