//! Explain every JavaScript load error with the PR's additions, then require real green tests.
use super::{attribution, compile, git};
use crate::ship::duel::focus::{CheckReport, Scope};
use std::{collections::BTreeSet, path::Path};

pub(super) fn is_js(error: &crate::ship::duel::focus::CompileError) -> bool {
    matches!(error.code.as_deref(), Some("JS_MODULE" | "JS_EXPORT"))
}

fn affected(tree: &Path, paths: &[String], error: &str) -> git::Result<Vec<String>> {
    if attribution::is_spec(error) {
        if !paths.iter().any(|p| p == error) {
            return Err(format!(
                "red: Playwright load error outside changed specs: {error}"
            ));
        }
        return Ok(vec![error.into()]);
    }
    // Match red-run's selection: explicit specs take precedence over a helper's broad scope.
    let selected: Vec<_> = if paths.iter().any(|p| attribution::is_spec(p)) {
        paths
            .iter()
            .filter(|p| attribution::is_spec(p))
            .cloned()
            .collect()
    } else {
        paths.to_vec()
    };
    let scope = Scope::from_paths(tree, &selected)?;
    let specs: Vec<_> = scope
        .specs
        .into_iter()
        .filter(|spec| {
            attribution::imports_changed(tree, spec, &[error.into()], &mut BTreeSet::new())
        })
        .collect();
    if specs.is_empty() {
        return Err(format!(
            "red: changed helper {error} is not imported by a selected spec"
        ));
    }
    Ok(specs)
}

pub(super) fn missing(
    root: &Path,
    base: &str,
    head: &str,
    red_tree: &Path,
    paths: &[String],
    report: &CheckReport,
) -> git::Result<Vec<String>> {
    if !report.compile_errors.iter().any(is_js) {
        return Ok(Vec::new());
    }
    if report.panics.iter().any(|p| {
        p.test
            .as_ref()
            .is_none_or(|name| !report.failed.contains(name))
    }) {
        return Err("red: a Playwright load error cannot explain a nameless/unowned panic".into());
    }
    let changed = git::git_raw(
        root,
        &["diff", "--name-only", "-z", "--no-renames", base, head],
    )?;
    let added = compile::added_identifiers(root, base, head, true)?;
    let mut missing = BTreeSet::new();
    for error in report.compile_errors.iter().filter(|e| is_js(e)) {
        if !changed.split('\0').any(|path| path == error.path) {
            return Err(format!(
                "red: Playwright load error outside changed specs/helpers: {}",
                error.path
            ));
        }
        affected(red_tree, paths, &error.path)?;
        if error.items.is_empty() {
            return Err("red: Playwright load error names no missing module/export".into());
        }
        for item in &error.items {
            let explained = if error.code.as_deref() == Some("JS_MODULE") {
                !super::is_test(item)
                    && git::git(root, &["cat-file", "-t", &format!("{head}:{item}")])
                        .is_ok_and(|kind| kind == "blob")
                    && git::git(root, &["cat-file", "-e", &format!("{base}:{item}")]).is_err()
            } else {
                added.contains(item)
            };
            if !explained {
                return Err(format!(
                    "red: Playwright load error at {} names a module/export the PR does not add: {item}",
                    error.path
                ));
            }
            missing.insert(item.clone());
        }
    }
    Ok(missing.into_iter().collect())
}

pub(super) fn ran(
    tree: &Path,
    base: &Path,
    paths: &[String],
    red: &CheckReport,
    green: &CheckReport,
) -> git::Result<Vec<String>> {
    let mut files = BTreeSet::new();
    for error in red.compile_errors.iter().filter(|e| is_js(e)) {
        files.extend(affected(tree, paths, &error.path)?);
    }
    let mut ran = BTreeSet::new();
    for file in files {
        let mut found = false;
        for name in green.tests.iter().filter(|n| !green.failed.contains(n)) {
            if green
                .locations
                .get(name)
                .is_some_and(|(path, _)| path == &file)
                && attribution::changed(tree, base, paths, green, name)?
            {
                ran.insert(name.clone());
                found = true;
            }
        }
        if !found {
            return Err(format!(
                "red: no changed test of {file} ran and passed at HEAD (filtered, skipped or not collected?)"
            ));
        }
    }
    Ok(ran.into_iter().collect())
}
