//! Reading the diff of an increment: how many lines it changes and which files
//! and lines a finding may cite. A path or hunk that cannot be read exactly
//! makes everything count as inside the increment, never as outside.
use super::{super::protocol::Reported, SMALL};
use std::collections::{BTreeMap, BTreeSet};

/// Where the lines of an increment are: a finding elsewhere is out of scope.
pub(in crate::review) struct Lines {
    /// New-side line ranges of the hunks of each file.
    ranges: BTreeMap<String, Vec<(u32, u32)>>,
    /// Every file the increment touches, hunks or not: deleted, renamed (old
    /// and new path), mode-changed and binary files too.
    files: BTreeSet<String>,
    /// A header that could not be read: everything counts as inside.
    unreadable: bool,
    /// Lines added plus removed; a binary or unreadable change is never small.
    changed: usize,
}

/// A `diff --git` block that has not named its file yet.
#[derive(Default)]
struct Block {
    open: bool,
    named: bool,
}

impl Lines {
    pub(in crate::review) fn of(diff: &str) -> Self {
        let mut me = Self {
            ranges: BTreeMap::new(),
            files: BTreeSet::new(),
            unreadable: false,
            changed: 0,
        };
        let (mut file, mut block) = (None::<String>, Block::default());
        // Lines still to come in the current hunk: (old side, new side).
        let mut left = (0u32, 0u32);
        for line in diff.lines() {
            if left != (0, 0) {
                match line.as_bytes().first() {
                    Some(b'+') => {
                        left.1 = left.1.saturating_sub(1);
                        me.changed += 1;
                    }
                    Some(b'-') => {
                        left.0 = left.0.saturating_sub(1);
                        me.changed += 1;
                    }
                    Some(b'\\') => {}
                    _ => {
                        left.0 = left.0.saturating_sub(1);
                        left.1 = left.1.saturating_sub(1);
                    }
                }
                continue;
            }
            if let Some(rest) = line.strip_prefix("diff --git ") {
                me.close(&block);
                block = Block {
                    open: true,
                    named: false,
                };
                file = None;
                match git_header(rest) {
                    Ok(paths) => block.named = me.add(paths),
                    Err(()) => me.unreadable = true,
                }
            } else if let Some(path) = line.strip_prefix("+++ ") {
                file = me.header(path, "b/", &mut block);
            } else if let Some(path) = line.strip_prefix("--- ") {
                me.header(path, "a/", &mut block);
            } else if let Some(path) = ["rename from ", "rename to ", "copy from ", "copy to "]
                .iter()
                .find_map(|key| line.strip_prefix(key))
            {
                match unquote(path) {
                    Some(path) => block.named |= me.add(vec![path]),
                    None => me.unreadable = true,
                }
            } else if let Some(hunk) = line.strip_prefix("@@ ") {
                match hunk_counts(hunk) {
                    Some((old, new, start)) => {
                        left = (old, new);
                        if let Some(path) = &file {
                            let end = start + new.saturating_sub(1);
                            me.ranges
                                .entry(path.clone())
                                .or_default()
                                .push((start, end));
                        }
                    }
                    None => {
                        me.unreadable = true;
                        me.changed += SMALL + 1;
                    }
                }
            } else if line.starts_with("Binary files") || line == "GIT binary patch" {
                me.changed += SMALL + 1;
            }
        }
        me.close(&block);
        me
    }

    /// The path of a `---` or `+++` header: remembered as a file of the
    /// increment, returned when it is a file with a new side to cite.
    fn header(&mut self, text: &str, prefix: &str, block: &mut Block) -> Option<String> {
        // Git appends a tab (and nothing else) to paths that contain spaces.
        let text = text.split('\t').next().unwrap_or(text);
        if text == "/dev/null" {
            return None;
        }
        let path = unquote(text).and_then(|p| p.strip_prefix(prefix).map(str::to_owned));
        match path {
            Some(path) => {
                block.named |= self.add(vec![path.clone()]);
                Some(path)
            }
            None => {
                self.unreadable = true;
                None
            }
        }
    }

    fn add(&mut self, paths: Vec<String>) -> bool {
        let named = !paths.is_empty();
        self.files.extend(paths);
        named
    }

    /// A block that never named its file cannot be placed: all inside.
    fn close(&mut self, block: &Block) {
        if block.open && !block.named {
            self.unreadable = true;
        }
    }

    /// Lines added plus removed by the diff.
    pub(super) fn changed(&self) -> usize {
        self.changed
    }

    /// Whether a finding at `file:line` is inside the increment: a line in a
    /// hunk of the file (with the context lines the reviewers saw), or any
    /// line, or none, of a file the increment touches without a hunk to cite
    /// (deleted, renamed, mode-changed or binary; for a rename, the old path
    /// too). Unreadable headers keep everything inside.
    pub(in crate::review) fn holds(&self, reported: &Reported) -> bool {
        if self.unreadable {
            return true;
        }
        let file = reported.file.trim_start_matches("./");
        match self.ranges.get(file) {
            Some(hunks) => reported
                .line
                .is_none_or(|n| hunks.iter().any(|&(from, to)| (from..=to).contains(&n))),
            None => self.files.contains(file),
        }
    }
}

/// Lines added plus removed; a binary change is never small.
pub(super) fn changed_lines(diff: &str) -> usize {
    Lines::of(diff).changed()
}

/// `-a[,b] +c[,d] @@`: the old and new line counts and the new start.
fn hunk_counts(hunk: &str) -> Option<(u32, u32, u32)> {
    let mut parts = hunk.split(' ');
    let old = parts.next()?.strip_prefix('-')?;
    let new = parts.next()?.strip_prefix('+')?;
    if parts.next()? != "@@" {
        return None;
    }
    let pair = |text: &str| -> Option<(u32, u32)> {
        let mut parts = text.split(',');
        let start = parts.next()?.parse().ok()?;
        let count = match parts.next() {
            Some(count) => count.parse().ok()?,
            None => 1,
        };
        parts.next().is_none().then_some((start, count))
    };
    Some((pair(old)?.1, pair(new)?.1, pair(new)?.0))
}

/// The paths named by the rest of a `diff --git` line: both when they are the
/// same, none when only the rename lines can tell, `Err` when unreadable.
fn git_header(rest: &str) -> Result<Vec<String>, ()> {
    let strip = |path: String, prefix: &str| path.strip_prefix(prefix).map(str::to_owned);
    if rest.starts_with('"') {
        let end = quoted_end(rest).ok_or(())?;
        let old = unquote(&rest[..end])
            .and_then(|p| strip(p, "a/"))
            .ok_or(())?;
        let new = rest[end..].trim_start();
        let new = unquote(new).and_then(|p| strip(p, "b/")).ok_or(())?;
        return Ok(vec![old, new]);
    }
    if rest.contains('"') {
        return Err(());
    }
    let rest = rest.as_bytes();
    let half = rest.len().saturating_sub(5) / 2;
    if rest.len() == 2 * half + 5
        && rest.starts_with(b"a/")
        && rest[2 + half..].starts_with(b" b/")
        && rest[2..2 + half] == rest[5 + half..]
    {
        let path = String::from_utf8(rest[2..2 + half].to_vec()).map_err(|_| ())?;
        return Ok(vec![path]);
    }
    Ok(Vec::new())
}

/// Index just past the closing quote of the quoted string at the start.
fn quoted_end(text: &str) -> Option<usize> {
    let mut escaped = false;
    for (i, c) in text.char_indices().skip(1) {
        match (escaped, c) {
            (true, _) => escaped = false,
            (false, '\\') => escaped = true,
            (false, '"') => return Some(i + 1),
            _ => {}
        }
    }
    None
}

/// A path as git prints it: plain, or quoted with C escapes and octal bytes.
/// `None` when it cannot be decoded exactly.
fn unquote(text: &str) -> Option<String> {
    let Some(inner) = text.strip_prefix('"') else {
        return Some(text.to_owned());
    };
    let inner = inner.strip_suffix('"')?.as_bytes();
    let (mut out, mut i) = (Vec::new(), 0);
    while i < inner.len() {
        let byte = inner[i];
        i += 1;
        if byte == b'"' {
            return None;
        }
        if byte != b'\\' {
            out.push(byte);
            continue;
        }
        let escape = *inner.get(i)?;
        i += 1;
        out.push(match escape {
            b'a' => 7,
            b'b' => 8,
            b'f' => 12,
            b'n' => b'\n',
            b'r' => b'\r',
            b't' => b'\t',
            b'v' => 11,
            b'\\' | b'"' => escape,
            b'0'..=b'3' => {
                let digits = inner.get(i..i + 2)?;
                if !digits.iter().all(|d| (b'0'..=b'7').contains(d)) {
                    return None;
                }
                i += 2;
                let octal = |d: u8| u32::from(d - b'0');
                let value = octal(escape) * 64 + octal(digits[0]) * 8 + octal(digits[1]);
                u8::try_from(value).ok()?
            }
            _ => return None,
        });
    }
    String::from_utf8(out).ok()
}
