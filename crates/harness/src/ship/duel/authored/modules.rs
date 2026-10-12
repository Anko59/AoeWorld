//! Module paths of unit-test files from the crate's real `mod`/`#[path]` tree.
use std::{
    fs,
    path::{Path, PathBuf},
};

/// nextest ids unit-test binaries `lib`, `<lib name>`, or `bin/<name>` for a binary-only crate.
pub(super) fn unit_target(manifest: &toml::Value, package: &str, target: &str) -> bool {
    let bins: Vec<&str> = manifest
        .get("bin")
        .and_then(|b| b.as_array())
        .map(|bins| {
            bins.iter()
                .filter_map(|b| b.get("name")?.as_str())
                .collect()
        })
        .unwrap_or_default();
    let name = target.strip_prefix("bin/").unwrap_or(target);
    target.is_empty()
        || target == "lib"
        || target == package
        || target == package.replace('-', "_")
        || manifest.get("lib").and_then(|l| l.get("name")?.as_str()) == Some(target)
        || bins.contains(&target)
        || name != target && (name == package || bins.contains(&name))
}

pub(super) fn crate_roots(directory: &Path, manifest: &toml::Value) -> Vec<PathBuf> {
    let mut roots = vec![directory.join("src/lib.rs"), directory.join("src/main.rs")];
    if let Ok(entries) = fs::read_dir(directory.join("src/bin")) {
        roots.extend(entries.flatten().map(|e| e.path()));
    }
    for table in ["lib", "bin"] {
        let entries = manifest.get(table).map(|t| match t.as_array() {
            Some(array) => array.clone(),
            None => vec![t.clone()],
        });
        for entry in entries.into_iter().flatten() {
            if let Some(path) = entry.get("path").and_then(|p| p.as_str()) {
                roots.push(directory.join(path));
            }
        }
    }
    roots.retain(|p| p.extension().is_some_and(|e| e == "rs") && p.is_file());
    roots
}

/// The module path of `file` in the crate's real tree, following `mod` and `#[path]`
/// from every crate root; `None` when no root reaches it.
pub(super) fn module_path(directory: &Path, manifest: &toml::Value, file: &Path) -> Option<String> {
    let file = normalize(file);
    crate_roots(directory, manifest)
        .into_iter()
        .find_map(|root| {
            if normalize(&root) == file {
                return Some(String::new());
            }
            let parsed = syn::parse_file(&fs::read_to_string(&root).ok()?).ok()?;
            let dir = root.parent()?;
            mount(&file, dir, dir, &parsed.items, "", 0)
        })
}

fn mount(
    target: &Path,
    file_dir: &Path,
    child_dir: &Path,
    items: &[syn::Item],
    prefix: &str,
    depth: usize,
) -> Option<String> {
    if depth > 32 {
        return None;
    }
    for item in items {
        let syn::Item::Mod(m) = item else { continue };
        let path = format!("{prefix}{}", m.ident);
        let inner = child_dir.join(m.ident.to_string());
        if let Some((_, children)) = &m.content {
            let nested = format!("{path}::");
            if let Some(found) = mount(target, file_dir, &inner, children, &nested, depth + 1) {
                return Some(found);
            }
            continue;
        }
        let explicit = m.attrs.iter().find_map(|a| {
            if a.path().is_ident("path")
                && let syn::Meta::NameValue(v) = &a.meta
                && let syn::Expr::Lit(lit) = &v.value
                && let syn::Lit::Str(value) = &lit.lit
            {
                return Some(file_dir.join(value.value()));
            }
            None
        });
        let child = explicit.clone().or_else(|| {
            [
                child_dir.join(format!("{}.rs", m.ident)),
                inner.join("mod.rs"),
            ]
            .into_iter()
            .find(|p| p.is_file())
        });
        let Some(child) = child.map(|c| normalize(&c)) else {
            continue;
        };
        if child == target {
            return Some(path);
        }
        let Some(parsed) = fs::read_to_string(&child)
            .ok()
            .and_then(|s| syn::parse_file(&s).ok())
        else {
            continue;
        };
        let dir = child.parent()?;
        let own = if explicit.is_some() || child.file_name()? == "mod.rs" {
            dir.to_owned()
        } else {
            child.with_extension("")
        };
        let nested = format!("{path}::");
        if let Some(found) = mount(target, dir, &own, &parsed.items, &nested, depth + 1) {
            return Some(found);
        }
    }
    None
}

fn normalize(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => (),
            std::path::Component::ParentDir => {
                result.pop();
            }
            other => result.push(other),
        }
    }
    result
}
