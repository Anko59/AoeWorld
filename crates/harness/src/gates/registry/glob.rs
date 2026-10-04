use super::helpers::{valid_path, valid_pattern};

/// Segment-aware glob: * and ? never cross /; a whole ** segment matches
/// zero or more segments. DP bounds work; Unicode ? matches one scalar value.
pub(super) fn glob(pattern: &str, path: &str) -> bool {
    if !valid_pattern(pattern) || !valid_path(path) {
        return false;
    }
    let pattern: Vec<_> = pattern.split('/').collect();
    let path: Vec<_> = path.split('/').collect();
    let mut row = vec![false; path.len() + 1];
    row[0] = true;
    for segment in pattern {
        let mut next = vec![false; row.len()];
        if segment == "**" {
            next[0] = row[0];
            for j in 1..next.len() {
                next[j] = row[j] || next[j - 1];
            }
        } else {
            for j in 1..next.len() {
                next[j] = row[j - 1] && segment_glob(segment, path[j - 1]);
            }
        }
        row = next;
    }
    row[path.len()]
}

fn segment_glob(pattern: &str, text: &str) -> bool {
    let text: Vec<_> = text.chars().collect();
    let mut row = vec![false; text.len() + 1];
    row[0] = true;
    for token in pattern.chars() {
        let mut next = vec![false; row.len()];
        if token == '*' {
            next[0] = row[0];
        }
        for j in 1..next.len() {
            next[j] = if token == '*' {
                row[j] || next[j - 1]
            } else {
                row[j - 1] && (token == '?' || token == text[j - 1])
            };
        }
        row = next;
    }
    row[text.len()]
}
