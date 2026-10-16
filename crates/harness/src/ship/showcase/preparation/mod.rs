//! Avoid another npm install for an unchanged, fully installed lock graph.
//! Image builds still run; checks such as browser-check keep their full path.
use serde_json::Value;
use std::{
    fs,
    path::{Component, Path, PathBuf},
};

pub(super) fn modules(root: &Path) -> PathBuf {
    std::env::var_os("AOE_SHOWCASE_NODE_MODULES")
        .map_or_else(|| root.join("browser/node_modules"), PathBuf::from)
}

fn json(path: &Path) -> Option<Value> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

fn installed(root: &Path, modules: &Path) -> Option<()> {
    let manifest = json(&root.join("browser/package.json"))?;
    let lock = json(&root.join("browser/package-lock.json"))?;
    let hidden = json(&modules.join(".package-lock.json"))?;
    if lock.get("lockfileVersion")?.as_u64()? != 3
        || hidden.get("lockfileVersion") != lock.get("lockfileVersion")
    {
        return None;
    }
    let mut packages = lock.get("packages")?.as_object()?.clone();
    let declared = packages.remove("")?;
    for field in ["dependencies", "devDependencies", "optionalDependencies"] {
        if manifest.get(field) != declared.get(field) {
            return None;
        }
    }
    if packages.is_empty() || hidden.get("packages")?.as_object()? != &packages {
        return None;
    }
    for (name, package) in &packages {
        let relative = Path::new(name).strip_prefix("node_modules").ok()?;
        if relative.as_os_str().is_empty()
            || relative
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
        {
            return None;
        }
        let version = package.get("version")?.as_str()?;
        let installed = json(&modules.join(relative).join("package.json"))?;
        if installed.get("version")?.as_str()? != version {
            return None;
        }
    }
    Some(())
}

pub(super) fn ready(root: &Path, modules: &Path) -> bool {
    installed(root, modules).is_some()
}
