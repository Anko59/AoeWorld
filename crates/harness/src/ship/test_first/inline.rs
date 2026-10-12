//! Locate test-only Rust items and test attributes, retaining source line spans.
use proc_macro2::{Delimiter, TokenStream, TokenTree};
use syn::{parse::Parser, spanned::Spanned};

fn mentions_test(tokens: TokenStream) -> bool {
    tokens.into_iter().any(|token| match token {
        TokenTree::Ident(ident) => ident == "test",
        TokenTree::Group(group) => mentions_test(group.stream()),
        _ => false,
    })
}

fn regions(tokens: TokenStream, cfg: bool, found: &mut Vec<(usize, usize)>) {
    let tokens: Vec<_> = tokens.into_iter().collect();
    for (i, token) in tokens.iter().enumerate() {
        if let TokenTree::Group(group) = token {
            regions(group.stream(), cfg, found);
        }
        if !matches!(token, TokenTree::Punct(p) if p.as_char() == '#') {
            continue;
        }
        let Some(TokenTree::Group(attr)) = tokens.get(i + 1) else {
            continue;
        };
        if attr.delimiter() != Delimiter::Bracket {
            continue;
        }
        let text = format!("#[{}]", attr.stream());
        let is_cfg =
            matches!(attr.stream().into_iter().next(), Some(TokenTree::Ident(id)) if id == "cfg");
        if (cfg && is_cfg && mentions_test(attr.stream()))
            || (!cfg && super::test_marker(&text.replace(' ', "")))
        {
            // Parse one item so braces in literals, attributes or generics cannot end it early.
            let mut start = i;
            while start >= 2
                && matches!(&tokens[start - 2], TokenTree::Punct(p) if p.as_char() == '#')
                && matches!(&tokens[start - 1], TokenTree::Group(g) if g.delimiter() == Delimiter::Bracket)
            {
                start -= 2;
            }
            let rest: TokenStream = tokens[start..].iter().cloned().collect();
            let parser = |input: syn::parse::ParseStream<'_>| {
                let item: syn::Item = input.parse()?;
                let _: TokenStream = input.parse()?;
                Ok(item)
            };
            let end = parser
                .parse2(rest)
                .map(|item| item.span().end().line)
                .unwrap_or_else(|_| {
                    tokens
                        .last()
                        .map_or(token.span().end().line, |last| last.span().end().line)
                });
            found.push((tokens[start].span().start().line, end));
        }
    }
}

fn source_regions(text: &str, cfg: bool) -> Vec<(usize, usize)> {
    let mut found = Vec::new();
    if let Ok(tokens) = text.parse::<TokenStream>() {
        regions(tokens, cfg, &mut found);
    }
    found
}

/// One-based inclusive line ranges of items under a `cfg` mentioning `test`.
pub(crate) fn test_regions(text: &str) -> Vec<(usize, usize)> {
    source_regions(text, true)
}

/// One-based inclusive ranges of whole `#[test]`-style items, including qualified paths.
pub(crate) fn test_attributes(text: &str) -> Vec<(usize, usize)> {
    source_regions(text, false)
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
    parse_changes(patch, false)
}

/// Include blank changed lines when enforcing byte-identical test code.
pub(crate) fn all_changes(patch: &str) -> Vec<(String, Changes)> {
    parse_changes(patch, true)
}

fn parse_changes(patch: &str, include_blank: bool) -> Vec<(String, Changes)> {
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
                if include_blank || !blank {
                    file.1.old.push(old);
                }
                old += 1;
            } else if line.starts_with('+') {
                if include_blank || !blank {
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
