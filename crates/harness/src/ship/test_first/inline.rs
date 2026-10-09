//! Inline tests: which changed lines of a Rust file lie inside `#[cfg(test)]`
//! items. Braces are counted line by line after `//` comments are cut; braces
//! inside strings or character literals can misplace an item's end, which the
//! report tolerates because reviewers judge it.

/// An attribute line opening a test-only item.
fn cfg_test(line: &str) -> bool {
    let compact: String = line.chars().filter(|c| !c.is_whitespace()).collect();
    compact.starts_with("#[cfg(test)]") || compact.starts_with("#[cfg(all(test,")
}

/// The index of the line closing the item whose attribute is on `start`.
fn item_end(lines: &[&str], start: usize) -> usize {
    let mut depth = 0usize;
    let mut opened = false;
    for (index, line) in lines.iter().enumerate().skip(start) {
        let code = line.split("//").next().unwrap_or_default();
        let code = if index == start {
            code.split_once(")]").map_or("", |(_, rest)| rest)
        } else {
            code
        };
        for c in code.chars() {
            match c {
                '{' => {
                    depth += 1;
                    opened = true;
                }
                '}' => depth = depth.saturating_sub(1),
                ';' if !opened => return index,
                _ => {}
            }
            if opened && depth == 0 {
                return index;
            }
        }
    }
    lines.len().saturating_sub(1)
}

/// One-based inclusive line ranges of the `#[cfg(test)]` items in `text`.
pub(crate) fn test_regions(text: &str) -> Vec<(usize, usize)> {
    let lines: Vec<&str> = text.lines().collect();
    let mut regions = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        if cfg_test(lines[index]) {
            let end = item_end(&lines, index);
            regions.push((index + 1, end + 1));
            index = end + 1;
        } else {
            index += 1;
        }
    }
    regions
}

/// Changed non-blank lines of one file in a `-U0` patch, by side.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Changes {
    pub(crate) old: Vec<usize>,
    pub(crate) new: Vec<usize>,
}

/// `-a,b +c,d` of a hunk header: the first old and new line numbers.
fn hunk_start(header: &str) -> Option<(usize, usize)> {
    let mut fields = header.strip_prefix("@@ ")?.split(' ');
    let start = |field: Option<&str>, sign: char| -> Option<usize> {
        let range = field?.strip_prefix(sign)?;
        range.split(',').next()?.parse().ok()
    };
    Some((start(fields.next(), '-')?, start(fields.next(), '+')?))
}

/// Per-file changes of a `git diff -U0` patch, by path (the new path, or the
/// old one for a deletion).
pub(crate) fn changes(patch: &str) -> Vec<(String, Changes)> {
    let mut files: Vec<(String, Changes)> = Vec::new();
    let (mut old, mut new) = (0usize, 0usize);
    let mut deleted_path = String::new();
    let mut header = false;
    for line in patch.lines() {
        if line.starts_with("diff --git ") {
            files.push((String::new(), Changes::default()));
            header = true;
        } else if header && line.starts_with("--- ") {
            deleted_path = line.strip_prefix("--- a/").unwrap_or_default().to_owned();
        } else if let Some(path) = line.strip_prefix("+++ ").filter(|_| header) {
            let path = path.strip_prefix("b/").unwrap_or(&deleted_path);
            if let Some(file) = files.last_mut() {
                file.0 = path.to_owned();
            }
        } else if let Some((o, n)) = hunk_start(line) {
            (old, new) = (o, n);
            header = false;
        } else if let (Some(file), Some(text)) = (files.last_mut(), line.get(1..)) {
            let blank = text.trim().is_empty();
            if line.starts_with('-') {
                if !blank {
                    file.1.old.push(old);
                }
                old += 1;
            } else if line.starts_with('+') {
                if !blank {
                    file.1.new.push(new);
                }
                new += 1;
            }
        }
    }
    files
}

/// Whether each changed line is inside a test item: `(test, product)` counts.
pub(crate) fn split(changes: &Changes, before: &str, after: &str) -> (usize, usize) {
    let inside = |regions: &[(usize, usize)], line: usize| {
        regions.iter().any(|(a, b)| (*a..=*b).contains(&line))
    };
    let (old, new) = (test_regions(before), test_regions(after));
    let tests = changes.old.iter().filter(|l| inside(&old, **l)).count()
        + changes.new.iter().filter(|l| inside(&new, **l)).count();
    (tests, changes.old.len() + changes.new.len() - tests)
}
