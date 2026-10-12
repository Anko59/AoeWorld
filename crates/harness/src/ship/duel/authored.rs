//! Red requires a failure identified in a file changed by this round's tester.
use super::focus::CheckReport;
use crate::ship::git;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[cfg(test)]
pub(super) fn has_failure(
    root: &Path,
    paths: &[String],
    report: &CheckReport,
) -> git::Result<bool> {
    evidence(root, paths, report).map(|(authored, _)| authored)
}

pub(super) fn evidence(
    root: &Path,
    paths: &[String],
    report: &CheckReport,
) -> git::Result<(bool, Vec<String>)> {
    let mut notes = Vec::new();
    let authored = has_failure_with_notes(root, paths, report, &mut notes)?;
    Ok((authored, notes))
}

fn has_failure_with_notes(
    root: &Path,
    paths: &[String],
    report: &CheckReport,
    notes: &mut Vec<String>,
) -> git::Result<bool> {
    for name in &report.failed {
        if report
            .locations
            .get(name)
            .is_some_and(|(path, _)| paths.contains(path))
        {
            return Ok(true);
        }
        // nextest JSON uses binary-id$test-path; human output separates them by space.
        let identity = name.split_once('$').or_else(|| name.split_once(' '));
        for path in paths.iter().filter(|p| p.ends_with(".rs")) {
            let source = match fs::read_to_string(root.join(path)) {
                Ok(source) => source,
                Err(_) => continue,
            };
            let Ok(parsed) = syn::parse_file(&source) else {
                continue;
            };
            let mut functions = Vec::new();
            test_functions(&parsed.items, "", &mut functions);
            let Some((binary, function)) = identity else {
                // Trusted overrides may provide only a function name, as in existing provenance.
                if functions
                    .iter()
                    .filter(|f| f.rsplit("::").next() == Some(name.as_str()))
                    .count()
                    == 1
                {
                    let mut owner = root.join(path);
                    while owner.pop() && owner.starts_with(root) {
                        if owner.join("Cargo.toml").is_file() {
                            if function_count(&root.join("crates"), name)? == 1 {
                                return Ok(true);
                            }
                            break;
                        }
                    }
                }
                continue;
            };
            let (package, target) = binary.split_once("::").unwrap_or((binary, ""));
            let mut directory = root.join(path);
            while directory.pop() && directory.starts_with(root) {
                let manifest = directory.join("Cargo.toml");
                if !manifest.is_file() {
                    continue;
                }
                let value: toml::Value =
                    toml::from_str(&fs::read_to_string(&manifest).map_err(|e| e.to_string())?)
                        .map_err(|e| e.to_string())?;
                if value
                    .get("package")
                    .and_then(|p| p.get("name"))
                    .and_then(|n| n.as_str())
                    != Some(package)
                {
                    break;
                }
                let relative = root
                    .join(path)
                    .strip_prefix(&directory)
                    .map_err(|e| e.to_string())?
                    .to_string_lossy()
                    .into_owned();
                if let Some(test) = relative.strip_prefix("tests/") {
                    let target_path = integration_target(&directory, &value, target);
                    if let Some(defining) = target_path
                        .as_ref()
                        .and_then(|path| defining_file(path, function.trim()))
                    {
                        if defining
                            .strip_prefix(root)
                            .ok()
                            .is_some_and(|p| paths.iter().any(|changed| Path::new(changed) == p))
                        {
                            return Ok(true);
                        }
                        break;
                    }
                    let binary = test
                        .split('/')
                        .next()
                        .unwrap_or(test)
                        .trim_end_matches(".rs");
                    if target == binary {
                        notes.push(format!(
                            "Source unresolved for {name}; falling back to integration target match: {path}"
                        ));
                        return Ok(true);
                    }
                } else if let Some(module) = relative.strip_prefix("src/") {
                    let module = module
                        .trim_end_matches(".rs")
                        .trim_end_matches("/mod")
                        .replace('/', "::");
                    let unit_target = target.is_empty()
                        || target == "lib"
                        || target == package
                        || target == package.replace('-', "_")
                        || value
                            .get("lib")
                            .and_then(|l| l.get("name"))
                            .and_then(|n| n.as_str())
                            == Some(target)
                        || value
                            .get("bin")
                            .and_then(|b| b.as_array())
                            .is_some_and(|bins| {
                                bins.iter()
                                    .any(|b| b.get("name").and_then(|n| n.as_str()) == Some(target))
                            });
                    if unit_target
                        && functions
                            .iter()
                            .any(|f| function.trim() == format!("{module}::{f}"))
                    {
                        return Ok(true);
                    }
                }
                break;
            }
        }
    }
    Ok(false)
}

fn integration_target(directory: &Path, manifest: &toml::Value, target: &str) -> Option<PathBuf> {
    if let Some(path) = manifest
        .get("test")
        .and_then(|v| v.as_array())
        .and_then(|tests| {
            tests
                .iter()
                .find(|t| t.get("name").and_then(|v| v.as_str()) == Some(target))
                .and_then(|t| t.get("path").and_then(|v| v.as_str()))
        })
    {
        return Some(directory.join(path));
    }
    [
        directory.join(format!("tests/{target}.rs")),
        directory.join(format!("tests/{target}/main.rs")),
    ]
    .into_iter()
    .find(|p| p.is_file())
}

/// Follow declared inline/external modules; unresolved macro-generated tests use the target fallback.
fn defining_file(path: &Path, function: &str) -> Option<PathBuf> {
    let parsed = syn::parse_file(&fs::read_to_string(path).ok()?).ok()?;
    let modules: Vec<_> = function.split("::").collect();
    resolve_items(path, path.parent()?, &parsed.items, &modules)
}

fn resolve_items(
    path: &Path,
    directory: &Path,
    items: &[syn::Item],
    names: &[&str],
) -> Option<PathBuf> {
    let (name, rest) = names.split_first()?;
    for item in items {
        match item {
            syn::Item::Fn(f) if rest.is_empty() && f.sig.ident == *name => {
                return Some(path.to_owned());
            }
            syn::Item::Mod(m) if !rest.is_empty() && m.ident == *name => {
                let child_directory = directory.join(name);
                if let Some((_, children)) = &m.content {
                    return resolve_items(path, &child_directory, children, rest);
                }
                let explicit = m.attrs.iter().find_map(|a| {
                    if a.path().is_ident("path")
                        && let syn::Meta::NameValue(v) = &a.meta
                        && let syn::Expr::Lit(lit) = &v.value
                        && let syn::Lit::Str(value) = &lit.lit
                    {
                        return Some(directory.join(value.value()));
                    }
                    None
                });
                let child = explicit.or_else(|| {
                    [
                        directory.join(format!("{name}.rs")),
                        child_directory.join("mod.rs"),
                    ]
                    .into_iter()
                    .find(|p| p.is_file())
                })?;
                let parsed = syn::parse_file(&fs::read_to_string(&child).ok()?).ok()?;
                let directory = if child.file_name()? == "mod.rs" {
                    child.parent()?.to_owned()
                } else {
                    child.with_extension("")
                };
                return resolve_items(&child, &directory, &parsed.items, rest);
            }
            _ => (),
        }
    }
    None
}

fn function_count(directory: &Path, name: &str) -> git::Result<usize> {
    let mut count = 0;
    for entry in fs::read_dir(directory).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.is_symlink() {
            continue;
        }
        if path.is_dir() {
            if !matches!(
                path.file_name().and_then(|n| n.to_str()),
                Some("target" | ".cache" | ".git")
            ) {
                count += function_count(&path, name)?;
            }
        } else if path.extension().is_some_and(|e| e == "rs") {
            let source = fs::read_to_string(path).map_err(|e| e.to_string())?;
            if let Ok(parsed) = syn::parse_file(&source) {
                let mut functions = Vec::new();
                test_functions(&parsed.items, "", &mut functions);
                count += functions
                    .iter()
                    .filter(|f| f.rsplit("::").next() == Some(name))
                    .count();
            }
        }
    }
    Ok(count)
}

fn test_functions(items: &[syn::Item], prefix: &str, functions: &mut Vec<String>) {
    for item in items {
        match item {
            syn::Item::Fn(f)
                if f.attrs.iter().any(|a| {
                    a.path()
                        .segments
                        .last()
                        .is_some_and(|s| s.ident.to_string().contains("test"))
                }) =>
            {
                functions.push(format!("{prefix}{}", f.sig.ident));
            }
            syn::Item::Mod(m) => {
                if let Some((_, items)) = &m.content {
                    test_functions(items, &format!("{prefix}{}::", m.ident), functions);
                }
            }
            _ => (),
        }
    }
}
