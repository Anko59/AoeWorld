//! A changed Rust test file is compiled only where a `mod` line mounts it, and that line lives in
//! a product file. The red tree gets exactly those declarations, never another product line.
use super::{git, is_test};
use std::{
    fs,
    path::{Component, Path, PathBuf},
};
use syn::spanned::Spanned;

/// Mount each changed test file in the red tree (`target`, the merge base plus the overlaid
/// tests) the way `head` does, and unmount the ones `head` deleted.
pub(super) fn apply(
    root: &Path,
    base: &str,
    head: &str,
    target: &Path,
    paths: &[String],
) -> git::Result<()> {
    for path in paths.iter().filter(|p| p.ends_with(".rs")) {
        if exists(root, head, path) {
            mount(root, base, head, target, path)?;
        } else if exists(root, base, path) {
            unmount(root, base, head, target, path)?;
        }
    }
    Ok(())
}

/// Declare `module` in its parent. A parent the merge base lacks is a stub holding only the
/// declaration of its child, itself mounted in turn, so the tests of a new module still build.
fn mount(root: &Path, base: &str, head: &str, target: &Path, module: &str) -> git::Result<()> {
    let Some((parent, text)) = declaration(root, head, module)? else {
        return Ok(());
    };
    let red = target.join(&parent);
    let at_base = show(root, base, &parent);
    if let Some(source) = &at_base
        && !mounts_of(source, &parent, module).is_empty()
    {
        return Ok(());
    }
    let mut source = fs::read_to_string(&red).unwrap_or_default();
    if mounts_of(&source, &parent, module).is_empty() {
        source.push_str(&format!("\n{text}\n"));
        if let Some(directory) = red.parent() {
            fs::create_dir_all(directory).map_err(|e| e.to_string())?;
        }
        fs::write(&red, source).map_err(|e| e.to_string())?;
    }
    if at_base.is_none() {
        mount(root, base, head, target, &parent)?;
    }
    Ok(())
}

/// Remove the declaration of a test file `head` deleted, when the product parent dropped it too.
fn unmount(root: &Path, base: &str, head: &str, target: &Path, module: &str) -> git::Result<()> {
    let Some((parent, _)) = declaration(root, base, module)? else {
        return Ok(());
    };
    if show(root, head, &parent).is_none_or(|s| !mounts_of(&s, &parent, module).is_empty()) {
        return Ok(());
    }
    let red = target.join(&parent);
    let Ok(source) = fs::read_to_string(&red) else {
        return Ok(());
    };
    // Back to front, so earlier ranges stay valid.
    let mut kept = source.clone();
    for range in mounts_of(&source, &parent, module).into_iter().rev() {
        kept.replace_range(range, "");
    }
    fs::write(&red, kept).map_err(|e| e.to_string())
}

/// The product file at `rev` that declares `module`, and the declaration as written.
fn declaration(root: &Path, rev: &str, module: &str) -> git::Result<Option<(String, String)>> {
    for parent in candidates(root, rev, module)? {
        if parent == module || is_test(&parent) {
            continue;
        }
        let Some(source) = show(root, rev, &parent) else {
            continue;
        };
        if let Some(range) = mounts_of(&source, &parent, module).into_iter().next() {
            return Ok(Some((parent, source[range].to_owned())));
        }
    }
    Ok(None)
}

/// The Rust files beside `module` and in each directory above it up to its crate root.
fn candidates(root: &Path, rev: &str, module: &str) -> git::Result<Vec<String>> {
    let mut found = Vec::new();
    let mut directory = Path::new(module).parent();
    while let Some(current) = directory.filter(|d| !d.as_os_str().is_empty()) {
        let prefix = format!("{}/", current.display());
        let listing = git::git_raw(root, &["ls-tree", "--name-only", "-z", rev, "--", &prefix])?;
        let files: Vec<&str> = listing.split('\0').filter(|f| !f.is_empty()).collect();
        found.extend(
            files
                .iter()
                .filter(|f| f.ends_with(".rs"))
                .map(|f| (*f).to_owned()),
        );
        if files.contains(&format!("{prefix}Cargo.toml").as_str()) {
            break;
        }
        directory = current.parent();
    }
    Ok(found)
}

/// Byte ranges of the `mod name;` items of `source` (the file `parent`) that load `module`.
fn mounts_of(source: &str, parent: &str, module: &str) -> Vec<std::ops::Range<usize>> {
    let Ok(file) = syn::parse_file(source) else {
        return Vec::new();
    };
    file.items
        .iter()
        .filter_map(|item| match item {
            syn::Item::Mod(m) if m.content.is_none() => Some(m),
            _ => None,
        })
        .filter(|m| loads(parent, m).iter().any(|p| p == module))
        .map(|m| {
            let (start, end) = (m.span().start(), m.span().end());
            offset(source, start.line, start.column)..offset(source, end.line, end.column)
        })
        .collect()
}

/// The files `mod name;` in `parent` may load.
fn loads(parent: &str, item: &syn::ItemMod) -> Vec<String> {
    let parent = Path::new(parent);
    let directory = parent.parent().unwrap_or(Path::new(""));
    let path = item.attrs.iter().find_map(|a| match &a.meta {
        syn::Meta::NameValue(nv) if nv.path.is_ident("path") => match &nv.value {
            syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(s),
                ..
            }) => Some(s.value()),
            _ => None,
        },
        _ => None,
    });
    let names = if let Some(path) = path {
        vec![directory.join(path)]
    } else {
        let owner = match parent.file_stem().and_then(|s| s.to_str()) {
            Some("mod" | "lib" | "main") => directory.to_path_buf(),
            Some(stem) => directory.join(stem),
            None => return Vec::new(),
        };
        let name = item.ident.to_string();
        vec![
            owner.join(format!("{name}.rs")),
            owner.join(&name).join("mod.rs"),
        ]
    };
    names.iter().map(|p| normalize(p)).collect()
}

fn normalize(path: &Path) -> String {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => (),
            other => out.push(other),
        }
    }
    out.to_string_lossy().into_owned()
}

/// The byte offset of a 1-based line and 0-based char column.
fn offset(source: &str, line: usize, column: usize) -> usize {
    let before: usize = source
        .split_inclusive('\n')
        .take(line - 1)
        .map(str::len)
        .sum();
    let text = source[before..].split_inclusive('\n').next().unwrap_or("");
    before
        + text
            .char_indices()
            .nth(column)
            .map_or(text.len(), |(i, _)| i)
}

fn exists(root: &Path, rev: &str, path: &str) -> bool {
    git::git(root, &["cat-file", "-e", &format!("{rev}:{path}")]).is_ok()
}

fn show(root: &Path, rev: &str, path: &str) -> Option<String> {
    git::git_raw(root, &["show", &format!("{rev}:{path}")]).ok()
}
