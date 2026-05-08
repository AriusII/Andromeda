use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum JsonField {
    Null,
    String(String),
    Unsigned(u64),
    Bool(bool),
}

pub(crate) fn escape_json_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            c if c.is_control() => {
                escaped.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => escaped.push(c),
        }
    }
    escaped
}

pub(crate) fn parse_flat_json_object(input: &str) -> Result<HashMap<String, JsonField>, String> {
    let mut parser = Parser::new(input);
    parser.parse_object()
}

pub(crate) fn required_string(
    fields: &HashMap<String, JsonField>,
    name: &str,
) -> Result<String, String> {
    match fields.get(name) {
        Some(JsonField::String(value)) => Ok(value.clone()),
        _ => Err(format!("missing {name}")),
    }
}

pub(crate) fn optional_string(
    fields: &HashMap<String, JsonField>,
    name: &str,
) -> Result<Option<String>, String> {
    match fields.get(name) {
        Some(JsonField::String(value)) => Ok(Some(value.clone())),
        Some(JsonField::Null) | None => Ok(None),
        _ => Err(format!("invalid {name}")),
    }
}

pub(crate) fn required_u64(fields: &HashMap<String, JsonField>, name: &str) -> Result<u64, String> {
    match fields.get(name) {
        Some(JsonField::Unsigned(value)) => Ok(*value),
        _ => Err(format!("missing {name}")),
    }
}

struct Parser<'a> {
    input: &'a [u8],
    index: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input: input.as_bytes(),
            index: 0,
        }
    }

    fn parse_object(&mut self) -> Result<HashMap<String, JsonField>, String> {
        self.skip_ws();
        self.expect_byte(b'{')?;
        let mut fields = HashMap::new();
        loop {
            self.skip_ws();
            if self.consume_byte(b'}') {
                break;
            }

            let key = self.parse_string()?;
            self.skip_ws();
            self.expect_byte(b':')?;
            let value = self.parse_value()?;
            fields.insert(key, value);
            self.skip_ws();

            if self.consume_byte(b',') {
                continue;
            }
            self.expect_byte(b'}')?;
            break;
        }
        self.skip_ws();
        if self.index != self.input.len() {
            return Err("trailing data after JSON object".to_string());
        }
        Ok(fields)
    }

    fn parse_value(&mut self) -> Result<JsonField, String> {
        self.skip_ws();
        match self.peek_byte() {
            Some(b'"') => self.parse_string().map(JsonField::String),
            Some(b'0'..=b'9') => self.parse_unsigned().map(JsonField::Unsigned),
            Some(b'n') => {
                self.expect_literal(b"null")?;
                Ok(JsonField::Null)
            }
            Some(b't') => {
                self.expect_literal(b"true")?;
                Ok(JsonField::Bool(true))
            }
            Some(b'f') => {
                self.expect_literal(b"false")?;
                Ok(JsonField::Bool(false))
            }
            _ => Err("invalid JSON value".to_string()),
        }
    }

    fn parse_string(&mut self) -> Result<String, String> {
        self.expect_byte(b'"')?;
        let mut value = String::new();
        while let Some(byte) = self.next_byte() {
            match byte {
                b'"' => return Ok(value),
                b'\\' => value.push(self.parse_escape()?),
                b if b < 0x20 => return Err("control character in JSON string".to_string()),
                b => value.push(b as char),
            }
        }
        Err("unterminated JSON string".to_string())
    }

    fn parse_escape(&mut self) -> Result<char, String> {
        match self.next_byte() {
            Some(b'"') => Ok('"'),
            Some(b'\\') => Ok('\\'),
            Some(b'/') => Ok('/'),
            Some(b'b') => Ok('\u{0008}'),
            Some(b'f') => Ok('\u{000c}'),
            Some(b'n') => Ok('\n'),
            Some(b'r') => Ok('\r'),
            Some(b't') => Ok('\t'),
            Some(b'u') => self.parse_unicode_escape(),
            _ => Err("invalid JSON escape".to_string()),
        }
    }

    fn parse_unicode_escape(&mut self) -> Result<char, String> {
        let mut value = 0u32;
        for _ in 0..4 {
            let Some(byte) = self.next_byte() else {
                return Err("incomplete unicode escape".to_string());
            };
            value = (value << 4)
                | match byte {
                    b'0'..=b'9' => u32::from(byte - b'0'),
                    b'a'..=b'f' => u32::from(byte - b'a' + 10),
                    b'A'..=b'F' => u32::from(byte - b'A' + 10),
                    _ => return Err("invalid unicode escape".to_string()),
                };
        }
        char::from_u32(value).ok_or_else(|| "invalid unicode scalar".to_string())
    }

    fn parse_unsigned(&mut self) -> Result<u64, String> {
        let start = self.index;
        while matches!(self.peek_byte(), Some(b'0'..=b'9')) {
            self.index += 1;
        }
        std::str::from_utf8(&self.input[start..self.index])
            .map_err(|_| "invalid number".to_string())?
            .parse::<u64>()
            .map_err(|_| "invalid unsigned integer".to_string())
    }

    fn expect_literal(&mut self, literal: &[u8]) -> Result<(), String> {
        if self.input.get(self.index..self.index + literal.len()) == Some(literal) {
            self.index += literal.len();
            Ok(())
        } else {
            Err("invalid JSON literal".to_string())
        }
    }

    fn expect_byte(&mut self, expected: u8) -> Result<(), String> {
        match self.next_byte() {
            Some(actual) if actual == expected => Ok(()),
            _ => Err(format!("expected `{}`", expected as char)),
        }
    }

    fn consume_byte(&mut self, expected: u8) -> bool {
        if self.peek_byte() == Some(expected) {
            self.index += 1;
            true
        } else {
            false
        }
    }

    fn next_byte(&mut self) -> Option<u8> {
        let byte = self.peek_byte()?;
        self.index += 1;
        Some(byte)
    }

    fn peek_byte(&self) -> Option<u8> {
        self.input.get(self.index).copied()
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek_byte(), Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.index += 1;
        }
    }
}
