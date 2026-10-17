//! Conservative source provenance for balanced Playwright test declarations.
//! Unknown or malformed syntax stays unidentified.
use std::ops::Range;

#[derive(Clone)]
struct Token<'a> {
    text: &'a str,
    range: Range<usize>,
    string: bool,
}

fn tokens(source: &str) -> Option<Vec<Token<'_>>> {
    let bytes = source.as_bytes();
    let mut result = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let start = i;
        match bytes[i] {
            b if b.is_ascii_whitespace() => {
                i += 1;
                continue;
            }
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i + 1 < bytes.len() && &bytes[i..i + 2] != b"*/" {
                    i += 1;
                }
                if i + 1 >= bytes.len() {
                    return None;
                }
                i += 2;
                continue;
            }
            b'/' => {
                let previous = result.last().map(|t: &Token<'_>| t.text);
                if previous.is_none_or(|s| {
                    matches!(
                        s,
                        "(" | "["
                            | "{"
                            | ":"
                            | ","
                            | "="
                            | ";"
                            | "return"
                            | "!"
                            | "?"
                            | "|"
                            | "&"
                            | ">"
                    )
                }) {
                    i = regex_end(bytes, i)?;
                    result.push(Token {
                        text: &source[start..i],
                        range: start..i,
                        string: true,
                    });
                    continue;
                }
                if previous.is_some_and(|s| matches!(s, ")" | "]" | "}")) {
                    return None;
                }
                i += 1;
            }
            b'`' => {
                i = quoted_end(bytes, i, 0)?;
                result.push(Token {
                    text: &source[start..i],
                    range: start..i,
                    string: true,
                });
                continue;
            }
            b'\'' | b'"' => {
                let quote = bytes[i];
                i += 1;
                while i < bytes.len() && bytes[i] != quote {
                    if bytes[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
                if i >= bytes.len() {
                    return None;
                }
                i += 1;
                result.push(Token {
                    text: &source[start..i],
                    range: start..i,
                    string: true,
                });
                continue;
            }
            b if b.is_ascii_alphanumeric() || b == b'_' || b == b'$' => {
                while i < bytes.len()
                    && (bytes[i].is_ascii_alphanumeric() || matches!(bytes[i], b'_' | b'$'))
                {
                    i += 1;
                }
            }
            b if b.is_ascii() => i += 1,
            _ => return None,
        }
        result.push(Token {
            text: &source[start..i],
            range: start..i,
            string: false,
        });
    }
    Some(result)
}

fn calls(source: &str) -> Option<Vec<(String, Range<usize>)>> {
    let tokens = tokens(source)?;
    let mut result = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        if tokens[i].text != "test" || tokens[i].string || (i > 0 && tokens[i - 1].text == ".") {
            i += 1;
            continue;
        }
        let start = i;
        i += 1;
        if tokens.get(i).is_some_and(|t| t.text == ".") {
            i += 1;
            if !tokens
                .get(i)
                .is_some_and(|t| matches!(t.text, "only" | "skip" | "fixme"))
            {
                continue;
            }
            i += 1;
        }
        if tokens.get(i).is_none_or(|t| t.text != "(") {
            continue;
        }
        let title_start = tokens.get(i + 1)?.range.start;
        let mut title_end = None;
        let mut stack = Vec::new();
        loop {
            let token = tokens.get(i)?;
            if !token.string {
                match token.text {
                    "," if stack.len() == 1 && title_end.is_none() => {
                        title_end = Some(token.range.start)
                    }
                    "(" => stack.push(")"),
                    "[" => stack.push("]"),
                    "{" => stack.push("}"),
                    ")" | "]" | "}" => {
                        if stack.pop()? != token.text {
                            return None;
                        }
                    }
                    _ => (),
                }
            }
            i += 1;
            if stack.is_empty() {
                let mut end = token.range.end;
                if tokens.get(i).is_some_and(|t| t.text == ";") {
                    end = tokens[i].range.end;
                    i += 1;
                }
                let title = &source[title_start..title_end.unwrap_or(token.range.start)];
                result.push((title.to_owned(), tokens[start].range.start..end));
                break;
            }
        }
    }
    Some(result)
}

pub(super) fn definitions(source: &str) -> Option<Vec<(String, usize)>> {
    Some(
        calls(source)?
            .into_iter()
            .map(|(title, range)| {
                (
                    title,
                    source[..range.start]
                        .bytes()
                        .filter(|b| *b == b'\n')
                        .count()
                        + 1,
                )
            })
            .collect(),
    )
}
fn rest<'a>(source: &'a str, calls: &[(String, Range<usize>)]) -> Option<Vec<&'a str>> {
    Some(
        tokens(source)?
            .into_iter()
            .filter(|t| !calls.iter().any(|(_, r)| r.contains(&t.range.start)))
            .map(|t| t.text)
            .collect(),
    )
}

pub(crate) fn unchanged(current: &str, old: &str, line: usize) -> bool {
    let (Some(current_calls), Some(old_calls)) = (calls(current), calls(old)) else {
        return false;
    };
    let Some((title, range)) = current_calls
        .iter()
        .find(|(_, r)| current[..r.start].bytes().filter(|b| *b == b'\n').count() + 1 == line)
    else {
        return false;
    };
    let matches: Vec<_> = old_calls.iter().filter(|(t, _)| t == title).collect();
    matches.len() == 1
        && current[range.clone()] == old[matches[0].1.clone()]
        && rest(current, &current_calls) == rest(old, &old_calls)
}

/// A JSON location must land on a parsed declaration, even for a new spec.
pub(in crate::ship) fn identified(source: &str, line: usize) -> bool {
    definitions(source).is_some_and(|definitions| definitions.iter().any(|(_, at)| *at == line))
}

/// Static relative imports/re-exports, lexed so comments and quoted examples
/// cannot attribute unrelated specs to a changed helper.
pub(in crate::ship) fn imports(source: &str) -> Option<Vec<String>> {
    let tokens = tokens(source)?;
    Some(
        tokens
            .windows(2)
            .filter_map(|pair| {
                let [keyword, path] = pair else { return None };
                if !keyword.string && matches!(keyword.text, "from" | "import") && path.string {
                    let text = path.text;
                    if (text.starts_with('\'') && text.ends_with('\''))
                        || (text.starts_with('"') && text.ends_with('"'))
                    {
                        return Some(text[1..text.len() - 1].to_owned());
                    }
                }
                None
            })
            .collect(),
    )
}

fn regex_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut i = start + 1;
    let mut class = false;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 1,
            b'[' => class = true,
            b']' => class = false,
            b'/' if !class => {
                i += 1;
                while bytes.get(i).is_some_and(u8::is_ascii_alphabetic) {
                    i += 1;
                }
                return Some(i);
            }
            b'\n' | b'\r' => return None,
            _ => (),
        }
        i += 1;
    }
    None
}

fn quoted_end(bytes: &[u8], start: usize, depth: usize) -> Option<usize> {
    if depth > 64 {
        return None;
    }
    let quote = *bytes.get(start)?;
    let mut i = start + 1;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if bytes[i] == quote {
            return Some(i + 1);
        }
        if quote == b'`' && bytes.get(i..i + 2) == Some(b"${") {
            i += 2;
            let mut braces = 1;
            while braces > 0 {
                match *bytes.get(i)? {
                    b'\'' | b'"' | b'`' => {
                        i = quoted_end(bytes, i, depth + 1)?;
                        continue;
                    }
                    b'{' => braces += 1,
                    b'}' => braces -= 1,
                    b'/' => return None,
                    _ => (),
                }
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    None
}
