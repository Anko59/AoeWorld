//! A file change alone cannot earn a named test's red proof.
use super::git;
use crate::ship::duel::{authored, focus::CheckReport, specs, wasm};
use std::{collections::BTreeSet, fs, path::Path};

pub(super) fn changed(
    root: &Path,
    old: &Path,
    paths: &[String],
    report: &CheckReport,
    name: &str,
) -> git::Result<bool> {
    if let Some((path, line)) = report.locations.get(name) {
        let Ok(current) = fs::read_to_string(root.join(path)) else {
            return Ok(false);
        };
        let before = fs::read_to_string(old.join(path)).ok();
        // The runner's JSON location must identify a real test declaration.
        if !specs::identified(&current, *line) {
            return Ok(false);
        }
        if paths.contains(path)
            && before
                .as_deref()
                .is_none_or(|before| !specs::unchanged(&current, before, *line))
        {
            return Ok(true);
        }
        return Ok(imports_changed(root, path, paths, &mut BTreeSet::new()));
    }
    authored::changed_failure(root, old, paths, name)
}

pub(super) fn imports_changed(
    root: &Path,
    path: &str,
    changed: &[String],
    visited: &mut BTreeSet<String>,
) -> bool {
    if visited.len() >= 256 || !visited.insert(path.into()) {
        return false;
    }
    let Some(imports) = fs::read_to_string(root.join(path))
        .ok()
        .and_then(|s| specs::imports(&s))
    else {
        return false;
    };
    for import in imports.iter().filter(|p| p.starts_with('.')) {
        let Some(directory) = Path::new(path).parent() else {
            continue;
        };
        let dependency = wasm::normalize(&directory.join(import).to_string_lossy());
        let source = Path::new(&dependency).extension().and_then(|extension| {
            let extension = match extension.to_str()? {
                "js" => "ts",
                "mjs" => "mts",
                "cjs" => "cts",
                _ => return None,
            };
            Some(Path::new(&dependency).with_extension(extension))
        });
        let dependency = [
            Some(dependency.clone()),
            source.map(|path| path.to_string_lossy().into_owned()),
            Some(format!("{dependency}.ts")),
            Some(format!("{dependency}/index.ts")),
        ]
        .into_iter()
        .flatten()
        .find(|p| root.join(p).is_file());
        if let Some(dependency) = dependency
            && (changed.iter().any(|p| p == &dependency && !is_spec(p))
                || imports_changed(root, &dependency, changed, visited))
        {
            return true;
        }
    }
    false
}

pub(super) fn is_spec(path: &str) -> bool {
    path.ends_with(".spec.ts") || path.ends_with(".test.ts")
}
