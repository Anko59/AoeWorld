//! Red requires a failure identified in a file changed by this round's tester.
mod changed;
mod modules;
use self::modules::{crate_roots, module_path, unit_target};
use super::focus::CheckReport;
use crate::ship::git;
pub(in crate::ship) use changed::changed_failure;
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

pub(in crate::ship) fn evidence(
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
        // A failure the check marked untouched (a test the tester did not
        // change, even inside a file the tester edited) is pre-existing: it
        // is never this round's red evidence.
        if report.untouched.contains(name) {
            continue;
        }
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
                    if !unit_target(&value, package, target) {
                        break;
                    }
                    let function = function.trim();
                    match module_path(&directory, &value, &root.join(path)) {
                        Some(module) => {
                            if functions.iter().any(|f| {
                                function
                                    == if module.is_empty() {
                                        f.clone()
                                    } else {
                                        format!("{module}::{f}")
                                    }
                            }) {
                                return Ok(true);
                            }
                        }
                        None if crate_roots(&directory, &value).iter().any(|r| {
                            fs::read_to_string(r).is_ok_and(|s| syn::parse_file(&s).is_ok())
                        }) =>
                        {
                            // A readable tree that never mounts this file (macro or include!): match the test by its name.
                            if functions
                                .iter()
                                .any(|f| function.rsplit("::").next() == f.rsplit("::").next())
                            {
                                notes.push(format!(
                                    "Module of {path} unresolved for {name}; falling back to test function names"
                                ));
                                return Ok(true);
                            }
                        }
                        None => {
                            let module = module
                                .trim_end_matches(".rs")
                                .trim_end_matches("/mod")
                                .replace('/', "::");
                            if functions
                                .iter()
                                .any(|f| function == format!("{module}::{f}"))
                            {
                                return Ok(true);
                            }
                        }
                    }
                }
                break;
            }
        }
    }
    // A panic raised in a file the tester changed this round (a shared test helper) is red too,
    // unless a named failing test explains it: that test's verdict stands, and a pre-existing
    // (untouched) one never makes this round red, wherever its assertion panicked.
    let explained = !report.failed.is_empty()
        && report
            .failed
            .iter()
            .all(|name| report.untouched.contains(name));
    for panic in &report.panics {
        if panic
            .test
            .as_ref()
            .is_some_and(|name| report.untouched.contains(name))
            || (panic.test.is_none() && explained)
        {
            notes.push(format!(
                "the panic at {} belongs to a pre-existing failing test, not to this round",
                panic.location
            ));
            continue;
        }
        if paths.iter().any(|changed| {
            let changed = super::wasm::normalize(changed);
            panic.path == changed || panic.path.ends_with(&format!("/{changed}"))
        }) {
            return Ok(true);
        }
        notes.push(format!(
            "no tester-authored failing test: the run panicked at {} which this round did not change",
            panic.location
        ));
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

pub(super) fn functions(source: &str) -> git::Result<Vec<String>> {
    let parsed = syn::parse_file(source).map_err(|e| e.to_string())?;
    let mut functions = Vec::new();
    test_functions(&parsed.items, "", &mut functions);
    Ok(functions)
}

/// Tests this round added or changed that a focused run executes: no `#[ignore]`
/// and no `cfg` other than `test`. `old` is the file at the round's base.
pub(super) fn added_functions(source: &str, old: Option<&str>) -> git::Result<Vec<String>> {
    changed_tests(source, old, false)
}

/// Tests the PR added or changed, whatever their attribute stack: a red name must come from a
/// test that ran, and the proof's green run, not the source, shows an ignored test never did.
pub(super) fn changed_functions(source: &str, old: Option<&str>) -> git::Result<Vec<String>> {
    changed_tests(source, old, true)
}

fn changed_tests(source: &str, old: Option<&str>, gated_too: bool) -> git::Result<Vec<String>> {
    let parsed = syn::parse_file(source).map_err(|e| e.to_string())?;
    let before: Vec<(String, String)> = old
        .and_then(|old| {
            let parsed = syn::parse_file(old).ok()?;
            Some(bodies(old, &test_fns(&parsed.items)))
        })
        .unwrap_or_default();
    let current = test_fns(&parsed.items);
    Ok(bodies(source, &current)
        .into_iter()
        .zip(&current)
        .filter(|((name, body), test)| {
            (gated_too || !test.skipped) && !before.contains(&(name.clone(), body.clone()))
        })
        .map(|((name, _), _)| name)
        .collect())
}

fn bodies(source: &str, tests: &[TestFn]) -> Vec<(String, String)> {
    let lines: Vec<_> = source.lines().collect();
    tests
        .iter()
        .map(|t| (t.name.clone(), lines[t.start - 1..t.end].join("\n")))
        .collect()
}

/// A test function with its qualified name, source lines and whether a plain run skips it.
pub(super) struct TestFn {
    pub(super) name: String,
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) skipped: bool,
}

/// The one test-function detector: any attribute whose last segment contains "test".
pub(super) fn test_fns(items: &[syn::Item]) -> Vec<TestFn> {
    let mut result = Vec::new();
    collect_tests(items, "", false, &mut result);
    result
}

fn gated(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        a.path().is_ident("ignore")
            || a.path().is_ident("cfg")
                && a.meta
                    .require_list()
                    .is_ok_and(|list| list.tokens.to_string() != "test")
    })
}

fn collect_tests(items: &[syn::Item], prefix: &str, skipped: bool, result: &mut Vec<TestFn>) {
    use syn::spanned::Spanned;
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
                result.push(TestFn {
                    name: format!("{prefix}{}", f.sig.ident),
                    start: f.span().start().line,
                    end: f.span().end().line,
                    skipped: skipped || gated(&f.attrs),
                });
            }
            syn::Item::Mod(m) => {
                if let Some((_, items)) = &m.content {
                    let skipped = skipped || gated(&m.attrs);
                    collect_tests(items, &format!("{prefix}{}::", m.ident), skipped, result);
                }
            }
            _ => (),
        }
    }
}

fn test_functions(items: &[syn::Item], prefix: &str, functions: &mut Vec<String>) {
    let mut tests = Vec::new();
    collect_tests(items, prefix, false, &mut tests);
    functions.extend(tests.into_iter().map(|t| t.name));
}
