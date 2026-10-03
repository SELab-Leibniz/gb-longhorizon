//! JSON helpers: escaping and a couple of small serialisers.

/// Escape a string for inclusion in a JSON string literal.
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// The standard error body: `{"error": message, "code": code}`.
pub fn error_body(message: &str, code: &str) -> String {
    format!(
        "{{\"error\":\"{}\",\"code\":\"{}\"}}",
        escape(message),
        escape(code)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escaping() {
        assert_eq!(escape("a\"b\\c"), "a\\\"b\\\\c");
        assert_eq!(escape("x\u{1}"), "x\\u0001");
        assert_eq!(
            error_body("bad", "bad_request"),
            "{\"error\":\"bad\",\"code\":\"bad_request\"}"
        );
    }
}
