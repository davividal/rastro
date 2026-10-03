//! One value in a node's answer, in the shapes the document can carry.

use std::collections::BTreeMap;

use rastro_collector::{Observation, Xxh3Digest};

/// A value from a node's answer, as a tree.
///
/// **Typed rather than all text**, so `"1"` and `1` stay different states, as the RabbitMQ
/// facet's definition values do. **A non-integer number becomes text carrying its own
/// spelling**, because the format admits no floating point and rounding `0.85` would report a
/// setting the cluster does not have. Object keys are sorted by the map, so two answers that
/// differ only in the order the node printed them render the same bytes; list order is kept,
/// because in a pipeline's processors or a template's patterns the order is the meaning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiValue {
    Null,
    Boolean(bool),
    Integer(i64),
    Text(String),
    List(Vec<ApiValue>),
    Object(BTreeMap<String, ApiValue>),
}

impl ApiValue {
    /// One digest for the whole tree, taken over an encoding in which no two trees coincide.
    ///
    /// Every value is tagged with its kind and every text and collection with its length, so
    /// `["a,b"]` and `["a","b"]` cannot hash alike. Object keys are in the map's order, which is
    /// sorted, so the digest does not depend on the order the node printed them in.
    pub fn digest(&self) -> Xxh3Digest {
        let mut bytes = Vec::new();
        self.encode_into(&mut bytes);
        Xxh3Digest::of(&bytes)
    }

    fn encode_into(&self, bytes: &mut Vec<u8>) {
        let length = |bytes: &mut Vec<u8>, length: usize| {
            bytes.extend_from_slice(&(length as u64).to_be_bytes());
        };

        match self {
            Self::Null => bytes.push(0),
            Self::Boolean(flag) => bytes.extend_from_slice(&[1, u8::from(*flag)]),
            Self::Integer(number) => {
                bytes.push(2);
                bytes.extend_from_slice(&number.to_be_bytes());
            }
            Self::Text(text) => {
                bytes.push(3);
                length(bytes, text.len());
                bytes.extend_from_slice(text.as_bytes());
            }
            Self::List(items) => {
                bytes.push(4);
                length(bytes, items.len());
                for item in items {
                    item.encode_into(bytes);
                }
            }
            Self::Object(entries) => {
                bytes.push(5);
                length(bytes, entries.len());
                for (key, value) in entries {
                    length(bytes, key.len());
                    bytes.extend_from_slice(key.as_bytes());
                    value.encode_into(bytes);
                }
            }
        }
    }
}

impl From<&ApiValue> for Observation {
    fn from(value: &ApiValue) -> Self {
        match value {
            ApiValue::Null => Observation::null(),
            ApiValue::Boolean(flag) => Observation::boolean(*flag),
            ApiValue::Integer(number) => Observation::integer(*number),
            ApiValue::Text(text) => Observation::text(text),
            ApiValue::List(items) => Observation::list(items.iter().map(Observation::from)),
            ApiValue::Object(entries) => Observation::object(
                entries
                    .iter()
                    .map(|(key, value)| (key.as_str(), Observation::from(value))),
            ),
        }
    }
}
