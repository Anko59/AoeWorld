//! Build-only lexical compaction. Human-authored WGSL remains the source of truth.

pub fn compact(source: &str) -> Result<String, &'static str> {
    let bytes = source.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    let mut space = false;
    while index < bytes.len() {
        if bytes[index].is_ascii_whitespace() {
            space = true;
            index += 1;
            continue;
        }
        if bytes[index..].starts_with(b"//") {
            space = true;
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
            continue;
        }
        if bytes[index..].starts_with(b"/*") {
            space = true;
            index += 2;
            let mut depth = 1;
            while index < bytes.len() && depth != 0 {
                if bytes[index..].starts_with(b"/*") {
                    depth += 1;
                    index += 2;
                } else if bytes[index..].starts_with(b"*/") {
                    depth -= 1;
                    index += 2;
                } else {
                    index += 1;
                }
            }
            if depth != 0 {
                return Err("unclosed WGSL block comment");
            }
            continue;
        }
        if space && !output.is_empty() {
            output.push(b' ');
        }
        space = false;
        if bytes[index] == b'"' {
            // Preserve quoted text, escapes and comment markers byte-for-byte.
            let start = index;
            index += 1;
            let mut closed = false;
            while index < bytes.len() {
                match bytes[index] {
                    b'\\' => {
                        index += 1;
                        if index < bytes.len() {
                            index += 1;
                        }
                    }
                    b'"' => {
                        index += 1;
                        closed = true;
                        break;
                    }
                    _ => index += 1,
                }
            }
            if !closed {
                return Err("unclosed WGSL quoted text");
            }
            output.extend_from_slice(&bytes[start..index]);
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }
    // Only ASCII separators/comments were removed; UTF-8 payload is unchanged.
    String::from_utf8(output).map_err(|_| "invalid compacted WGSL UTF-8")
}

#[cfg(not(test))]
pub fn generate() -> Result<(), Box<dyn std::error::Error>> {
    use std::{env, fs, path::PathBuf};
    println!("cargo:rerun-if-changed=src/sprites.wgsl");
    println!("cargo:rerun-if-changed=build/shader.rs");
    println!("cargo:rerun-if-changed=build.rs");
    let manifest =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").ok_or("missing manifest directory")?);
    let source = fs::read_to_string(manifest.join("src/sprites.wgsl"))?;
    let compacted = compact(&source)?;
    let output = PathBuf::from(env::var_os("OUT_DIR").ok_or("missing build output directory")?);
    fs::write(
        output.join("sprites_shader.rs"),
        format!("const SHADER_SOURCE: &str = {compacted:?};\n"),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Independent token scan for the source/compacted golden. In particular,
    // contiguous operators must differ from operators separated by whitespace.
    fn tokens(source: &str) -> Vec<String> {
        let bytes = source.as_bytes();
        let mut result = Vec::new();
        let mut at = 0;
        while at < bytes.len() {
            if bytes[at].is_ascii_whitespace() {
                at += 1;
                continue;
            }
            if source[at..].starts_with("//") {
                at = source[at..].find('\n').map_or(bytes.len(), |end| at + end);
                continue;
            }
            if source[at..].starts_with("/*") {
                at += 2;
                let mut nesting = 1;
                while nesting != 0 {
                    assert!(at < bytes.len(), "unclosed oracle comment");
                    if bytes[at..].starts_with(b"/*") {
                        nesting += 1;
                        at += 2;
                    } else if bytes[at..].starts_with(b"*/") {
                        nesting -= 1;
                        at += 2;
                    } else {
                        at += 1;
                    }
                }
                continue;
            }
            let start = at;
            if bytes[at] == b'"' {
                at += 1;
                loop {
                    assert!(at < bytes.len(), "unclosed oracle quote");
                    let byte = bytes[at];
                    at += 1;
                    if byte == b'\\' {
                        at += 1;
                    } else if byte == b'"' {
                        break;
                    }
                }
            } else if bytes[at].is_ascii_digit()
                || (bytes[at] == b'.' && bytes.get(at + 1).is_some_and(u8::is_ascii_digit))
            {
                at += 1;
                while at < bytes.len() {
                    let byte = bytes[at];
                    if byte.is_ascii_alphanumeric()
                        || matches!(byte, b'_' | b'.')
                        || (matches!(byte, b'+' | b'-')
                            && matches!(bytes[at - 1], b'e' | b'E' | b'p' | b'P'))
                    {
                        at += 1;
                    } else {
                        break;
                    }
                }
            } else if bytes[at].is_ascii_alphabetic() || bytes[at] == b'_' {
                at += 1;
                while at < bytes.len() && (bytes[at].is_ascii_alphanumeric() || bytes[at] == b'_') {
                    at += 1;
                }
            } else {
                let operator = [
                    ">>=", "<<=", "==", "!=", ">=", "<=", "&&", "||", "++", "--", "->", "+=", "-=",
                    "*=", "/=", "%=", "&=", "|=", "^=", "<<", ">>",
                ]
                .into_iter()
                .find(|operator| source[at..].starts_with(*operator));
                let width = operator
                    .map_or_else(|| source[at..].chars().next().unwrap().len_utf8(), str::len);
                at += width;
            }
            result.push(source[start..at].to_owned());
        }
        result
    }

    #[test]
    fn whole_production_shader_has_identical_tokens_and_compaction_is_idempotent() {
        let original = include_str!("../src/sprites.wgsl");
        let minified = compact(original).unwrap();
        assert_eq!(tokens(original), tokens(&minified));
        assert_eq!(compact(&minified).unwrap(), minified);
        assert!(minified.len() < original.len());
    }

    #[test]
    fn operators_float_identifiers_comments_and_quote_payload_keep_boundaries() {
        for source in [
            "a > > b; a >> b; a / / b; a + + b; a ++ b;",
            "a/* nested /* inner */ tail */b// line\n0x1.fp+3 .5e-2 1.0f _name123",
            "name \t\r\n other >= >= 3u",
            r#""literal // /* spaces  ""#,
            r#""escaped \" quote // /*" suffix"#,
            "",
            "// only comment",
            "/* outer /* inner */ */",
        ] {
            let minified = compact(source).unwrap();
            assert_eq!(tokens(source), tokens(&minified), "source: {source}");
        }
        assert_eq!(compact("a/*gap*/b").unwrap(), "a b");
        assert_eq!(
            tokens("1.0f .5e-2 0x1.fp+3 _name123"),
            ["1.0f", ".5e-2", "0x1.fp+3", "_name123"]
        );
        assert_ne!(tokens("a > > b"), tokens("a >> b"));
    }

    #[test]
    fn unclosed_nested_comments_and_quotes_fail_build_compaction() {
        assert!(compact("/* outer /* closed inner */").is_err());
        assert!(compact("/*").is_err());
        assert!(compact("\"unclosed").is_err());
        assert!(compact("\"escaped trailing \\").is_err());
    }
}
