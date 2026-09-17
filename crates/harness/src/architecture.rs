//! Tested dependency and source-boundary rules for first-party crates.
use serde_json::Value;
use std::{collections::BTreeSet, error::Error, fs, path::Path, process::Command};

pub fn check(root: &Path) -> Result<(), Box<dyn Error>> {
    let output = Command::new("cargo")
        .args(["metadata", "--locked", "--no-deps", "--format-version", "1"])
        .current_dir(root)
        .output()?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string().into());
    }
    let metadata: Value = serde_json::from_slice(&output.stdout)?;
    let packages = metadata["packages"]
        .as_array()
        .ok_or("Cargo metadata has no packages")?;
    let mut violations = Vec::new();
    for package in packages {
        let name = package["name"].as_str().ok_or("package has no name")?;
        let dependencies: BTreeSet<_> = package["dependencies"]
            .as_array()
            .ok_or("package has no dependencies")?
            .iter()
            .filter_map(|dep| dep["name"].as_str())
            .collect();
        for dep in &dependencies {
            if forbidden(name, dep) {
                violations.push(format!("{name} may not depend on {dep}"));
            }
        }
        if name == "aoe-core" || name == "aoe-simulation" || name == "aoe-scenario" {
            let path = root
                .join("crates")
                .join(name.trim_start_matches("aoe-"))
                .join("src");
            for source in sources(&path)? {
                if test_only(&source)? {
                    continue;
                }
                let content = fs::read_to_string(&source)?;
                let production = content.split("#[cfg(test)]").next().unwrap_or(&content);
                for api in [
                    "std::fs",
                    "std::env",
                    "std::time",
                    "tokio::",
                    "web_sys::",
                    "js_sys::",
                    "wasm_bindgen::",
                ] {
                    if production.contains(api) {
                        violations.push(format!("{} uses boundary API {api}", source.display()));
                    }
                }
            }
        }
    }
    for source in sources(&root.join("crates"))? {
        if test_only(&source)? {
            continue;
        }
        let content = fs::read_to_string(&source)?;
        let production = content.split("#[cfg(test)]").next().unwrap_or(&content);
        for marker in [
            "unsafe {",
            "unsafe fn",
            ".unwrap()",
            ".expect(",
            "todo!",
            "unimplemented!",
            "panic!(",
        ] {
            if production.contains(marker) {
                violations.push(format!("{} contains production {marker}", source.display()));
            }
        }
    }
    if violations.is_empty() {
        Ok(())
    } else {
        Err(violations.join("\n").into())
    }
}

fn test_only(source: &Path) -> Result<bool, Box<dyn Error>> {
    if source.components().any(|part| part.as_os_str() == "tests") {
        return Ok(true);
    }
    if source.file_name().is_none_or(|name| name != "tests.rs") {
        return Ok(false);
    }
    let parent = source.parent().ok_or("test source has no parent")?;
    let module_name = parent.file_name().ok_or("test module has no name")?;
    let parent_module = parent.with_file_name(format!("{}.rs", module_name.to_string_lossy()));
    let declaration = fs::read_to_string(parent_module)?;
    if !declaration.contains("#[cfg(test)]\nmod tests;") {
        return Err("tests.rs must be declared behind #[cfg(test)]".into());
    }
    Ok(true)
}

fn forbidden(package: &str, dependency: &str) -> bool {
    if ["aoe-core", "aoe-scenario", "aoe-simulation"].contains(&package) {
        return [
            "tokio",
            "axum",
            "web-sys",
            "js-sys",
            "wasm-bindgen",
            "wgpu",
            "aoe-server",
            "aoe-client",
            "aoe-harness",
            "aoe-rendering",
            "aoe-protocol",
        ]
        .contains(&dependency);
    }
    if package != "aoe-harness" && dependency == "aoe-harness" {
        return true;
    }
    if package == "aoe-protocol"
        && ["aoe-simulation", "aoe-server", "aoe-client", "aoe-harness"].contains(&dependency)
    {
        return true;
    }
    false
}

fn sources(root: &Path) -> Result<Vec<std::path::PathBuf>, Box<dyn Error>> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut result = Vec::new();
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        if path.is_dir() {
            result.extend(sources(&path)?);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            result.push(path);
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dependency_direction_is_explicit() {
        assert!(forbidden("aoe-simulation", "tokio"));
        assert!(forbidden("aoe-protocol", "aoe-simulation"));
        assert!(forbidden("aoe-server", "aoe-harness"));
        assert!(!forbidden("aoe-server", "aoe-simulation"));
    }
}
