//! Quote-aware scans over already-lexed text: line continuations and command
//! substitutions nested inside one consumed unit (`${...}`, `$(( ))`).
use super::valid_name;

/// A backslash-newline that continues a line. Inside single quotes the pair is
/// literal data, so the quoting state is tracked before it counts: everywhere
/// else (unquoted or double quoted) the shell removes it as a continuation.
pub(super) fn continues_line(chars: &[char]) -> bool {
    let (mut single, mut double) = (false, false);
    let mut at = 0;
    while at < chars.len() {
        let c = chars[at];
        if single {
            single = c != '\'';
            at += 1;
        } else if c == '\\' {
            if chars.get(at + 1) == Some(&'\n') {
                return true;
            }
            at += 2;
        } else if double {
            double = c != '"';
            at += 1;
        } else {
            match c {
                '\'' => single = true,
                '"' => double = true,
                _ => {}
            }
            at += 1;
        }
    }
    false
}

/// Command substitutions (`$(...)`, backticks) inside text the lexer consumed
/// as one unit (the body of a `${...}` or `$(( ))` expansion). Each inner
/// command is queued so it is judged as a line of its own; nesting inside it
/// is handled when that line is judged.
pub(super) fn inner_substitutions(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut found = Vec::new();
    let mut at = 0;
    while at < chars.len() {
        match chars[at] {
            '$' if chars.get(at + 1) == Some(&'(') => {
                let start = at + 2;
                let mut depth = 1;
                let mut end = start;
                while end < chars.len() {
                    match chars[end] {
                        '(' => depth += 1,
                        ')' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                    end += 1;
                }
                if end < chars.len() {
                    found.push(chars[start..end].iter().collect());
                    at = end + 1;
                } else {
                    at = start;
                }
            }
            '`' => {
                let start = at + 1;
                match chars[start..].iter().position(|&c| c == '`') {
                    Some(end) => {
                        found.push(chars[start..start + end].iter().collect());
                        at = start + end + 1;
                    }
                    None => at = start,
                }
            }
            _ => at += 1,
        }
    }
    found
}

/// The parameter forms agents may expand: a name, `1`…`9` or `?$#@*`.
pub(super) fn plain_parameter(name: &str) -> bool {
    valid_name(name) || (name.len() == 1 && "123456789?$#@*".contains(name))
}
