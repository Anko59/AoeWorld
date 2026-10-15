//! Quote-aware scans over already-lexed text: line continuations and command
//! substitutions nested inside one consumed unit (`${...}`, `$(( ))`).
use super::valid_name;

/// A backslash-newline that continues a line. Inside single quotes the pair is
/// literal data, so the quoting state is tracked before it counts: everywhere
/// else (unquoted or double quoted) the shell removes it as a continuation.
/// A `#` that begins a word starts a comment that runs to the end of the line:
/// its text is data, so quotes and backslashes inside it are not tracked.
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
        } else if starts_comment(chars, at) {
            while chars.get(at).is_some_and(|c| *c != '\n') {
                at += 1;
            }
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

/// A `#` that begins a word starts a comment: at the start of the input or
/// after whitespace and the joining operators. Inside a word (`a#b`,
/// `${#x}`) or inside quotes it is plain data.
fn starts_comment(chars: &[char], at: usize) -> bool {
    chars[at] == '#'
        && (at == 0
            || matches!(
                chars[at - 1],
                ' ' | '\t' | '\r' | '\n' | ';' | '&' | '|' | '(' | ')'
            ))
}

/// Command substitutions (`$(...)`, backticks) inside text the lexer consumed
/// as one unit (the body of a `${...}` or `$(( ))` expansion). Each inner
/// command is queued so it is judged as a line of its own; nesting inside it
/// is handled when that line is judged. Quoting decides like bash does:
/// inside single quotes a substitution is literal data, while inside double
/// quotes (where a `'` is literal too) and after a backslash escape it still
/// runs only when unescaped.
///
/// `in_double` says the expansion itself sits inside double quotes: there an
/// apostrophe is an ordinary character (`"${x:-'$(cmd)'}"` runs `cmd`), so it
/// never starts a single-quoted span.
pub(super) fn inner_substitutions(text: &str, in_double: bool) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut found = Vec::new();
    let (mut single, mut double) = (false, false);
    let mut at = 0;
    while at < chars.len() {
        let c = chars[at];
        if single {
            single = c != '\'';
            at += 1;
        } else if c == '\\' {
            at += 2;
        } else {
            match c {
                '\'' if !double && !in_double => single = true,
                '"' => double = !double,
                '$' if chars.get(at + 1) == Some(&'(') => {
                    at = substitution(&chars, at, &mut found);
                    continue;
                }
                '`' => {
                    at = substitution(&chars, at, &mut found);
                    continue;
                }
                _ => {}
            }
            at += 1;
        }
    }
    found
}

/// The command substitution starting at `at` (`$(...)` or a backtick), added
/// to `found`, and the position just past its end (or past its `$`/backtick
/// when it never closes).
fn substitution(chars: &[char], at: usize, found: &mut Vec<String>) -> usize {
    if chars[at] != '$' {
        let start = at + 1;
        return match chars[start..].iter().position(|&c| c == '`') {
            Some(end) => {
                found.push(chars[start..start + end].iter().collect());
                start + end + 1
            }
            None => start,
        };
    }
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
        end + 1
    } else {
        start
    }
}

/// The parameter forms agents may expand: a name, `1`…`9` or `?$#@*`.
pub(super) fn plain_parameter(name: &str) -> bool {
    valid_name(name) || (name.len() == 1 && "123456789?$#@*".contains(name))
}
