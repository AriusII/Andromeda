//! JSON helpers for operator-facing CLI diagnostics only.
//!
//! This module is intentionally scoped to command-line diagnostic output. It is
//! not a runtime protocol, QUIC wire format, or stable storage encoding.

pub const JSON_FLAG: &str = "--json";
pub const DIAGNOSTIC_JSON_FLAG: &str = "--diagnostic-json";

pub fn json_option_u64(value: Option<u64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "null".to_string())
}

pub fn json_option_string(value: Option<&str>) -> String {
    value.map(json_string).unwrap_or_else(|| "null".to_string())
}

pub fn json_string(value: &str) -> String {
    format!("\"{}\"", escape_json_str(value))
}

pub fn json_string_array<T: AsRef<str>>(values: &[T]) -> String {
    let entries = values
        .iter()
        .map(|value| json_string(value.as_ref()))
        .collect::<Vec<_>>()
        .join(",");
    format!("[{}]", entries)
}

pub fn json_u64_array(values: &[u64]) -> String {
    let entries = values
        .iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(",");
    format!("[{}]", entries)
}

fn escape_json_str(value: &str) -> String {
    let mut escaped = String::new();
    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            ch if ch.is_control() => escaped.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => escaped.push(ch),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_string_escapes_diagnostic_fields() {
        assert_eq!(
            json_string("operator\tpath\\\"name\"\n\u{0007}"),
            "\"operator\\tpath\\\\\\\"name\\\"\\n\\u0007\""
        );
    }

    #[test]
    fn json_options_render_null_or_escaped_values() {
        assert_eq!(json_option_u64(Some(42)), "42");
        assert_eq!(json_option_u64(None), "null");
        assert_eq!(json_option_string(Some("a\"b")), "\"a\\\"b\"");
        assert_eq!(json_option_string(None), "null");
    }

    #[test]
    fn arrays_render_compact_json_literals() {
        assert_eq!(json_string_array(&["a", "b\\c"]), "[\"a\",\"b\\\\c\"]");
        assert_eq!(json_u64_array(&[1, 2, 3]), "[1,2,3]");
    }
}
