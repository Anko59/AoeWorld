//! Resolve changes by the full local test name, including inline modules.
use super::*;

pub(in crate::ship) fn changed_failure(
    root: &Path,
    old: &Path,
    paths: &[String],
    name: &str,
) -> git::Result<bool> {
    let function = name
        .split_once('$')
        .or_else(|| name.split_once(' '))
        .map_or(name, |(_, f)| f)
        .trim();
    for path in paths.iter().filter(|p| p.ends_with(".rs")) {
        let Ok(source) = fs::read_to_string(root.join(path)) else {
            continue;
        };
        let before = fs::read_to_string(old.join(path)).ok();
        let added = added_functions(&source, before.as_deref())?;
        let functions = functions(&source)?;
        // Prefer nested::case over case; matching only a leaf launders an
        // unchanged nested test beside a changed top-level test of that name.
        let local = functions
            .iter()
            .filter(|f| function == f.as_str() || function.ends_with(&format!("::{f}")))
            .max_by_key(|f| f.len());
        if let Some(local) = local
            && added.contains(local)
            && resolved(root, path, name, local)?
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn resolved(root: &Path, path: &str, name: &str, local: &str) -> git::Result<bool> {
    let Some((binary, function)) = name.split_once('$') else {
        return Ok(false);
    };
    let (package, target) = binary.split_once("::").unwrap_or((binary, ""));
    let file = root.join(path);
    let mut directory = file.clone();
    while directory.pop() && directory.starts_with(root) {
        let manifest = directory.join("Cargo.toml");
        if !manifest.is_file() {
            continue;
        }
        let value: toml::Value =
            toml::from_str(&fs::read_to_string(manifest).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        if value
            .get("package")
            .and_then(|p| p.get("name"))
            .and_then(|n| n.as_str())
            != Some(package)
        {
            return Ok(false);
        }
        if file
            .strip_prefix(&directory)
            .is_ok_and(|p| p.starts_with("tests"))
        {
            return Ok(integration_target(&directory, &value, target)
                .and_then(|entry| defining_file(&entry, function))
                .and_then(|found| found.canonicalize().ok())
                == file.canonicalize().ok());
        }
        return Ok(unit_target(&value, package, target)
            && module_path(&directory, &value, &file).is_some_and(|module| {
                function
                    == if module.is_empty() {
                        local.into()
                    } else {
                        format!("{module}::{local}")
                    }
            }));
    }
    Ok(false)
}
