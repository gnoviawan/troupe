//! JavaScript string and JSON measures the TypeScript store's bounds use:
//! `length` and `slice` count UTF-16 code units, and a serialized size is the
//! length of `JSON.stringify`.

use serde_json::Value;

/// `text.length`.
pub fn len(text: &str) -> usize {
    text.encode_utf16().count()
}

/// `text.slice(0, end)`. A surrogate pair cut in half by `end` loses its
/// first half too: Rust strings cannot hold the lone surrogate JavaScript
/// keeps.
pub fn head(text: &str, end: usize) -> &str {
    let mut units = 0;
    for (index, ch) in text.char_indices() {
        units += ch.len_utf16();
        if units > end {
            return &text[..index];
        }
    }
    text
}

/// `text.slice(-count)`. `slice(-0)` is `slice(0)`, the whole text.
pub fn tail(text: &str, count: usize) -> &str {
    if count == 0 {
        return text;
    }
    let mut units = 0;
    for (index, ch) in text.char_indices().rev() {
        units += ch.len_utf16();
        if units >= count {
            return if units == count {
                &text[index..]
            } else {
                &text[index + ch.len_utf8()..]
            };
        }
    }
    text
}

/// `JSON.stringify(value).length`; 0 for an absent value, as
/// `JSON.stringify(undefined)?.length ?? 0` is. Numbers count as JavaScript
/// prints them (`1`, not `1.0`).
pub fn serialized_len(value: Option<&Value>) -> usize {
    value.map_or(0, json_len)
}

fn json_len(value: &Value) -> usize {
    match value {
        Value::Null | Value::Bool(true) => 4,
        Value::Bool(false) => 5,
        Value::Number(number) => len(&number_text(number)),
        Value::String(text) => len(&Value::String(text.clone()).to_string()),
        Value::Array(items) => {
            2 + items.len().saturating_sub(1) + items.iter().map(json_len).sum::<usize>()
        }
        Value::Object(map) => {
            2 + map.len().saturating_sub(1)
                + map
                    .iter()
                    .map(|(key, value)| {
                        len(&Value::String(key.clone()).to_string()) + 1 + json_len(value)
                    })
                    .sum::<usize>()
        }
    }
}

fn number_text(number: &serde_json::Number) -> String {
    if number.is_f64() {
        number
            .as_f64()
            .map_or_else(|| number.to_string(), crate::js_number)
    } else {
        number.to_string()
    }
}

/// JavaScript truthiness: not `null`, `false`, `0`, `NaN` or `""`.
pub fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(flag) => *flag,
        Value::Number(number) => number.as_f64().is_some_and(|n| n != 0.0),
        Value::String(text) => !text.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

/// `String(value)` for the scalars a payload carries.
pub fn string_of(value: Option<&Value>) -> String {
    match value {
        None => "undefined".to_owned(),
        Some(Value::String(text)) => text.clone(),
        Some(Value::Number(number)) => number_text(number),
        Some(Value::Object(_)) => "[object Object]".to_owned(),
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| match item {
                Value::Null => String::new(),
                other => string_of(Some(other)),
            })
            .collect::<Vec<_>>()
            .join(","),
        Some(other) => other.to_string(),
    }
}

/// A finite JSON number, as `typeof x === 'number' && Number.isFinite(x)`.
pub fn finite(value: Option<&Value>) -> Option<f64> {
    value.and_then(Value::as_f64).filter(|n| n.is_finite())
}
