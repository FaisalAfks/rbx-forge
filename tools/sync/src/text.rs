//! Small TOML writing helpers: keys and strings. forge writes its TOML itself so every file has one fixed form
//! (decision 0015); the `toml` crate is only used to read.

/// A key as TOML: bare when it can be, quoted otherwise.
pub fn key(k: &str) -> String {
    let bare = !k.is_empty()
        && k.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if bare { k.to_string() } else { string(k) }
}

/// A TOML basic string, with every character TOML refuses escaped.
pub fn string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                out.push_str(&format!("\\u{:04X}", c as u32))
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_read_back() {
        for s in [
            "",
            "plain",
            "quote \" and \\ slash",
            "line\nbreak\r\ttab",
            "\u{1}\u{7f}",
            "ünï ✓ 日本",
        ] {
            let t: toml::Table = format!("v = {}", string(s)).parse().unwrap();
            assert_eq!(t["v"].as_str(), Some(s));
        }
    }

    #[test]
    fn keys_are_bare_only_when_safe() {
        assert_eq!(key("Size"), "Size");
        assert_eq!(key("a b"), "\"a b\"");
        assert_eq!(key("%54ext"), "\"%54ext\"");
        assert_eq!(key(""), "\"\"");
    }
}
