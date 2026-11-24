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

/// `JSON.stringify(value).length`; 0 for an absent value, as
/// `JSON.stringify(undefined)?.length ?? 0` is.
pub fn serialized_len(value: Option<&Value>) -> usize {
    value.map_or(0, |value| len(&value.to_string()))
}

/// `String(value)` for the scalars a payload carries.
pub fn string_of(value: Option<&Value>) -> String {
    match value {
        None => "undefined".to_owned(),
        Some(Value::String(text)) => text.clone(),
        Some(Value::Number(number)) => number
            .as_f64()
            .map_or_else(|| number.to_string(), crate::js_number),
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
