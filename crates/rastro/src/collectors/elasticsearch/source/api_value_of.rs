//! A node's JSON, as the collector's own value tree.

use crate::collectors::elasticsearch::value_objects::ApiValue;

/// `value` as an [`ApiValue`]: an integer stays one, any other number becomes its spelling.
pub fn api_value_of(value: &serde_json::Value) -> ApiValue {
    match value {
        serde_json::Value::Null => ApiValue::Null,
        serde_json::Value::Bool(flag) => ApiValue::Boolean(*flag),
        serde_json::Value::Number(number) => match number.as_i64() {
            Some(integer) => ApiValue::Integer(integer),
            None => ApiValue::Text(number.to_string()),
        },
        serde_json::Value::String(text) => ApiValue::Text(text.clone()),
        serde_json::Value::Array(items) => ApiValue::List(items.iter().map(api_value_of).collect()),
        serde_json::Value::Object(entries) => ApiValue::Object(
            entries
                .iter()
                .map(|(key, value)| (key.clone(), api_value_of(value)))
                .collect(),
        ),
    }
}
