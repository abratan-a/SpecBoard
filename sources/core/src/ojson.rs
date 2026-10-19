//! JSON written as the window writes it (`OJSON` in the Swift app's Export): keys in the order
//! they were added, `JSON.stringify`'s escapes and no more, an integral number without a
//! decimal point — pretty with two spaces, or compact.

use serde_json::{Number, Value};

/// Pretty, two spaces a level, as `JSON.stringify(value, null, 2)`. No newline at the end.
pub fn pretty(value: &Value) -> String {
    let mut out = String::new();
    write_pretty(value, "", &mut out);
    out
}

fn write_pretty(value: &Value, indent: &str, out: &mut String) {
    let inner = format!("{indent}  ");
    match value {
        Value::Array(items) => {
            if items.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push_str("[\n");
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(",\n");
                }
                out.push_str(&inner);
                write_pretty(item, &inner, out);
            }
            out.push('\n');
            out.push_str(indent);
            out.push(']');
        }
        Value::Object(object) => {
            if object.is_empty() {
                out.push_str("{}");
                return;
            }
            out.push_str("{\n");
            for (i, (key, item)) in object.iter().enumerate() {
                if i > 0 {
                    out.push_str(",\n");
                }
                out.push_str(&inner);
                out.push_str(&quote(key));
                out.push_str(": ");
                write_pretty(item, &inner, out);
            }
            out.push('\n');
            out.push_str(indent);
            out.push('}');
        }
        scalar => out.push_str(&scalar_text(scalar)),
    }
}

/// Without spacing, as `JSON.stringify(value)`.
pub fn compact(value: &Value) -> String {
    match value {
        Value::Array(items) => format!(
            "[{}]",
            items.iter().map(compact).collect::<Vec<_>>().join(",")
        ),
        Value::Object(object) => format!(
            "{{{}}}",
            object
                .iter()
                .map(|(k, v)| format!("{}:{}", quote(k), compact(v)))
                .collect::<Vec<_>>()
                .join(",")
        ),
        scalar => scalar_text(scalar),
    }
}

fn scalar_text(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => number(n),
        Value::String(s) => quote(s),
        _ => unreachable!("not a scalar"),
    }
}

/// A number as SP prints a `Double`: integral and below 1e15 without a decimal point;
/// otherwise the shortest text that reads back the same, in exponent form outside
/// the range Swift writes plainly.
pub fn number(n: &Number) -> String {
    let value = n.as_f64().unwrap_or(0.0);
    double(value)
}

pub fn double(value: f64) -> String {
    if !value.is_finite() {
        return "null".to_string();
    }
    if value == value.round() && value.abs() < 1e15 {
        return format!("{}", value as i64);
    }
    let magnitude = value.abs();
    if (1e-4..1e16).contains(&magnitude) {
        let text = format!("{value}");
        // Swift writes an integral double with `.0`.
        if text.contains('.') {
            text
        } else {
            format!("{text}.0")
        }
    } else {
        // `1e-05`, `1.5e+20`: a mantissa of the shortest digits, the exponent signed and
        // at least two digits.
        let text = format!("{value:e}");
        let (mantissa, exponent) = text.split_once('e').unwrap_or((&text, "0"));
        let exponent: i32 = exponent.parse().unwrap_or(0);
        let sign = if exponent < 0 { '-' } else { '+' };
        format!("{mantissa}e{sign}{:02}", exponent.abs())
    }
}

/// `JSON.stringify`'s escapes: the quote, the backslash, and control characters.
/// Anything else, Japanese included, as it is.
pub fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0C}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn written_as_the_window_writes_it() {
        let v = json!({"b": 3.0, "a": [1, "x\"y\n"], "c": {}, "d": []});
        assert_eq!(
            pretty(&v),
            "{\n  \"b\": 3,\n  \"a\": [\n    1,\n    \"x\\\"y\\n\"\n  ],\n  \"c\": {},\n  \"d\": []\n}"
        );
        assert_eq!(
            compact(&v),
            "{\"b\":3,\"a\":[1,\"x\\\"y\\n\"],\"c\":{},\"d\":[]}"
        );
    }

    #[test]
    fn numbers_as_swift_prints_them() {
        assert_eq!(double(0.5), "0.5");
        assert_eq!(double(-12.0), "-12");
        assert_eq!(double(0.00001), "1e-05");
        assert_eq!(double(1e20), "1e+20");
    }
}
