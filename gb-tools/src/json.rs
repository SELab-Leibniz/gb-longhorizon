//! A tiny hand-rolled JSON reader and writer, sufficient for the debugger
//! API (GEP Appendix B). Zero dependencies: no `serde`, on purpose.
//!
//! Only what the API needs is supported: objects, arrays, strings, integers
//! (also accepted/emitted as raw numbers), booleans and `null`. Numbers are
//! kept as `i64` because every number in the API is a JSON integer; a JSON
//! number with a fraction or exponent is parsed as a float and can be read
//! back as an integer when it is integral.

/// A parsed JSON value.
#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    /// `null`.
    Null,
    /// `true` / `false`.
    Bool(bool),
    /// A JSON number without a fraction or exponent.
    Int(i64),
    /// A JSON number that is not an integer.
    Float(f64),
    /// A string (UTF-8).
    Str(String),
    /// An array.
    Array(Vec<Json>),
    /// An object, preserving key order.
    Object(Vec<(String, Json)>),
}

impl Json {
    /// Look up a field of an object.
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Object(fields) => fields.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// The integer value of this node, if it is one.
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Json::Int(n) => Some(*n),
            Json::Float(f) if f.fract() == 0.0 => Some(*f as i64),
            _ => None,
        }
    }

    /// The boolean value of this node, if it is one.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Json::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// The string value of this node, if it is one.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s.as_str()),
            _ => None,
        }
    }

    /// The elements of this node, if it is an array.
    pub fn as_array(&self) -> Option<&[Json]> {
        match self {
            Json::Array(items) => Some(items),
            _ => None,
        }
    }

    fn write_to(&self, out: &mut String) {
        match self {
            Json::Null => out.push_str("null"),
            Json::Bool(true) => out.push_str("true"),
            Json::Bool(false) => out.push_str("false"),
            Json::Int(n) => out.push_str(&n.to_string()),
            Json::Float(f) => out.push_str(&format_float(*f)),
            Json::Str(s) => write_json_string(s, out),
            Json::Array(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    item.write_to(out);
                }
                out.push(']');
            }
            Json::Object(fields) => {
                out.push('{');
                for (i, (k, v)) in fields.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    write_json_string(k, out);
                    out.push(':');
                    v.write_to(out);
                }
                out.push('}');
            }
        }
    }
}

impl std::fmt::Display for Json {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut out = String::new();
        self.write_to(&mut out);
        f.write_str(&out)
    }
}

fn format_float(f: f64) -> String {
    if f.fract() == 0.0 && f.is_finite() && f.abs() < 1e15 {
        (f as i64).to_string()
    } else {
        f.to_string()
    }
}

fn write_json_string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Parse a complete JSON document. Trailing whitespace is allowed; trailing
/// non-whitespace is an error.
pub fn parse(input: &[u8]) -> Result<Json, String> {
    let mut p = Parser {
        data: input,
        pos: 0,
    };
    p.skip_ws();
    let value = p.value()?;
    p.skip_ws();
    if p.pos != p.data.len() {
        return Err("trailing characters after JSON value".into());
    }
    Ok(value)
}

struct Parser<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Parser<'_> {
    fn skip_ws(&mut self) {
        while let Some(&b) = self.data.get(self.pos) {
            if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn peek(&self) -> Option<u8> {
        self.data.get(self.pos).copied()
    }

    fn value(&mut self) -> Result<Json, String> {
        match self.peek() {
            None => Err("unexpected end of input".into()),
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => Ok(Json::Str(self.string()?)),
            Some(b't') | Some(b'f') => self.boolean(),
            Some(b'n') => self.null(),
            Some(b'-') | Some(b'0'..=b'9') => self.number(),
            Some(c) => Err(format!("unexpected byte 0x{c:02x}")),
        }
    }

    fn object(&mut self) -> Result<Json, String> {
        self.pos += 1; // '{'
        let mut fields = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(Json::Object(fields));
        }
        loop {
            self.skip_ws();
            if self.peek() != Some(b'"') {
                return Err("expected object key".into());
            }
            let key = self.string()?;
            self.skip_ws();
            if self.peek() != Some(b':') {
                return Err("expected ':'".into());
            }
            self.pos += 1;
            self.skip_ws();
            let value = self.value()?;
            fields.push((key, value));
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.pos += 1;
                }
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(Json::Object(fields));
                }
                _ => return Err("expected ',' or '}'".into()),
            }
        }
    }

    fn array(&mut self) -> Result<Json, String> {
        self.pos += 1; // '['
        let mut items = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(Json::Array(items));
        }
        loop {
            self.skip_ws();
            let value = self.value()?;
            items.push(value);
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.pos += 1;
                }
                Some(b']') => {
                    self.pos += 1;
                    return Ok(Json::Array(items));
                }
                _ => return Err("expected ',' or ']'".into()),
            }
        }
    }

    fn string(&mut self) -> Result<String, String> {
        self.pos += 1; // '"'
        let mut out = String::new();
        loop {
            let b = self.peek().ok_or("unterminated string")?;
            self.pos += 1;
            match b {
                b'"' => return Ok(out),
                b'\\' => {
                    let esc = self.peek().ok_or("unterminated escape")?;
                    self.pos += 1;
                    match esc {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{08}'),
                        b'f' => out.push('\u{0c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            if self.pos + 4 > self.data.len() {
                                return Err("bad \\u escape".into());
                            }
                            let hex = &self.data[self.pos..self.pos + 4];
                            self.pos += 4;
                            let code = std::str::from_utf8(hex)
                                .ok()
                                .and_then(|h| u32::from_str_radix(h, 16).ok())
                                .ok_or("bad \\u escape")?;
                            out.push(char::from_u32(code).unwrap_or('\u{fffd}'));
                        }
                        _ => return Err("bad escape".into()),
                    }
                }
                b if b < 0x20 => return Err("control character in string".into()),
                b => {
                    // Re-assemble UTF-8: take the raw byte and any continuation
                    // bytes that belong to the same character.
                    let start = self.pos - 1;
                    let len = utf8_len(b);
                    let end = start + len;
                    if len > 1 {
                        if end > self.data.len() {
                            return Err("truncated UTF-8".into());
                        }
                        self.pos = end;
                    }
                    let s = std::str::from_utf8(&self.data[start..end])
                        .map_err(|_| "invalid UTF-8".to_string())?;
                    out.push_str(s);
                }
            }
        }
    }

    fn boolean(&mut self) -> Result<Json, String> {
        if self.data[self.pos..].starts_with(b"true") {
            self.pos += 4;
            Ok(Json::Bool(true))
        } else if self.data[self.pos..].starts_with(b"false") {
            self.pos += 5;
            Ok(Json::Bool(false))
        } else {
            Err("invalid literal".into())
        }
    }

    fn null(&mut self) -> Result<Json, String> {
        if self.data[self.pos..].starts_with(b"null") {
            self.pos += 4;
            Ok(Json::Null)
        } else {
            Err("invalid literal".into())
        }
    }

    fn number(&mut self) -> Result<Json, String> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.pos += 1;
        }
        let mut float = false;
        if self.peek() == Some(b'.') {
            float = true;
            self.pos += 1;
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
        }
        if matches!(self.peek(), Some(b'e') | Some(b'E')) {
            float = true;
            self.pos += 1;
            if matches!(self.peek(), Some(b'+') | Some(b'-')) {
                self.pos += 1;
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
        }
        let text = std::str::from_utf8(&self.data[start..self.pos]).map_err(|_| "bad number")?;
        if float {
            text.parse::<f64>()
                .map(Json::Float)
                .map_err(|_| "bad number".to_string())
        } else {
            text.parse::<i64>()
                .map(Json::Int)
                .map_err(|_| "bad number".to_string())
        }
    }
}

fn utf8_len(b: u8) -> usize {
    if b < 0x80 {
        1
    } else if b >> 5 == 0b110 {
        2
    } else if b >> 4 == 0b1110 {
        3
    } else if b >> 3 == 0b11110 {
        4
    } else {
        // Invalid lead byte; treat as a single byte so the UTF-8 check later
        // reports the error.
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_object() {
        let src = br#"{"path":"/a.gb","model":"dmg","n":12,"ok":true}"#;
        let v = parse(src).unwrap();
        assert_eq!(v.get("model").and_then(Json::as_str), Some("dmg"));
        assert_eq!(v.get("n").and_then(Json::as_int), Some(12));
        assert_eq!(v.get("ok").and_then(Json::as_bool), Some(true));
        let text = v.to_string();
        assert_eq!(parse(text.as_bytes()).unwrap(), v);
    }

    #[test]
    fn arrays_and_escapes() {
        let v = parse(br#"{"buttons":["A","START"],"t":"a\"b\n"}"#).unwrap();
        let arr = v.get("buttons").and_then(Json::as_array).unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[1].as_str(), Some("START"));
        let text = v.to_string();
        assert_eq!(parse(text.as_bytes()).unwrap(), v);
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse(b"{").is_err());
        assert!(parse(b"nope").is_err());
        assert!(parse(b"{} extra").is_err());
    }

    #[test]
    fn int_and_float() {
        assert_eq!(parse(b"42").unwrap().as_int(), Some(42));
        assert_eq!(parse(b"2.0").unwrap().as_int(), Some(2));
        assert_eq!(parse(b"-3").unwrap().as_int(), Some(-3));
    }
}
