//! Single gate registry, documentation rendering, and path-based impact selection.
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    error::Error,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Deserialize)]
struct Registry {
    version: u16,
    gates: Vec<Gate>,
}

#[derive(Deserialize)]
struct Gate {
    id: String,
    command: String,
    requires: Vec<String>,
    select: String,
    evidence: String,
}

fn parse(bytes: &[u8]) -> Result<Registry, Box<dyn Error>> {
    let registry: Registry = serde_json::from_slice(bytes)?;
    if registry.version != 1 || registry.gates.is_empty() {
        return Err("unsupported or empty gate registry".into());
    }
    let mut seen = BTreeSet::new();
    for gate in &registry.gates {
        if !seen.insert(&gate.id) {
            return Err(format!("duplicate gate {}", gate.id).into());
        }
        if !gate.command.starts_with("make ") || gate.command.split_whitespace().count() != 2 {
            return Err(format!("invalid command for {}", gate.id).into());
        }
        if gate.command != format!("make {}", gate.id) {
            return Err(format!("gate command mismatch: {}", gate.id).into());
        }
    }
    for gate in &registry.gates {
        for prerequisite in &gate.requires {
            if !seen.contains(prerequisite) || prerequisite == &gate.id {
                return Err(format!("invalid dependency {prerequisite} for {}", gate.id).into());
            }
        }
    }
    Ok(registry)
}

fn table(registry: &Registry) -> String {
    let mut output = "# Implemented gate registry\n\nGenerated from `gates/registry.json`. `make docs-check` detects drift.\n\n| Gate | Command | Depends on | Selection | Evidence |\n|---|---|---|---|---|\n".to_owned();
    for gate in &registry.gates {
        let requires = if gate.requires.is_empty() {
            "—".to_owned()
        } else {
            gate.requires.join(", ")
        };
        output.push_str(&format!(
            "| {} | `{}` | {} | {} | {} |\n",
            gate.id, gate.command, requires, gate.select, gate.evidence
        ));
    }
    output
}

fn markdown_files(root: &Path) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    let mut result = Vec::new();
    for directory in [root.to_path_buf(), root.join("docs"), root.join("crates")] {
        collect_markdown(&directory, &mut result)?;
    }
    result.sort();
    result.dedup();
    Ok(result)
}

fn collect_markdown(directory: &Path, output: &mut Vec<PathBuf>) -> Result<(), Box<dyn Error>> {
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| {
                matches!(
                    name.to_str(),
                    Some(
                        "target" | ".git" | ".cache" | "node_modules" | "local-assets" | "reports"
                    )
                )
            }) {
                continue;
            }
            collect_markdown(&path, output)?;
        } else if path.extension().is_some_and(|ext| ext == "md") {
            output.push(path);
        }
    }
    Ok(())
}

fn local_links_exist(path: &Path) -> Result<(), Box<dyn Error>> {
    let content = fs::read_to_string(path)?;
    for tail in content.split("](").skip(1) {
        let Some(raw) = tail.split(')').next() else {
            continue;
        };
        if raw.starts_with("https://")
            || raw.starts_with("http://")
            || raw.starts_with('#')
            || raw.starts_with("mailto:")
        {
            continue;
        }
        let target = raw.split('#').next().unwrap_or(raw);
        if target.is_empty() {
            continue;
        }
        let resolved = path
            .parent()
            .ok_or("Markdown path has no parent")?
            .join(target);
        if !resolved.exists() {
            return Err(format!("{} links to missing {}", path.display(), target).into());
        }
    }
    Ok(())
}

pub fn docs_check(root: &Path) -> Result<(), Box<dyn Error>> {
    let registry = parse(&fs::read(root.join("gates/registry.json"))?)?;
    let expected = table(&registry);
    let actual = fs::read_to_string(root.join("docs/gates.md"))?;
    if actual != expected {
        return Err("docs/gates.md has drifted from registry".into());
    }
    for path in markdown_files(root)? {
        local_links_exist(&path)?;
    }
    Ok(())
}

#[derive(Debug, Serialize, Eq, PartialEq)]
pub struct Impact {
    pub suites: BTreeSet<String>,
    pub paths: Vec<String>,
}

fn classify(paths: &[String]) -> Impact {
    let all: BTreeSet<String> = [
        "static",
        "native",
        "browser",
        "assets",
        "performance",
        "release",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    let mut suites = BTreeSet::new();
    suites.insert("static".to_owned());
    for path in paths {
        if path.starts_with("docs/")
            || path.ends_with(".md")
            || path == "LICENSE"
            || path == "THIRD_PARTY.md"
        {
            continue;
        }
        if path.starts_with("browser/")
            || path.starts_with("web/")
            || path.starts_with("crates/client/")
            || path.starts_with("crates/rendering/")
        {
            suites.insert("browser".to_owned());
            suites.insert("native".to_owned());
        } else if path.starts_with("crates/assets/") {
            suites.insert("assets".to_owned());
            suites.insert("native".to_owned());
        } else if path.starts_with("crates/server/")
            || path.starts_with("crates/core/")
            || path.starts_with("crates/scenario/")
            || path.starts_with("crates/simulation/")
            || path.starts_with("crates/protocol/")
        {
            suites.extend(
                ["native", "browser", "performance"]
                    .into_iter()
                    .map(str::to_owned),
            );
        } else {
            return Impact {
                suites: all,
                paths: paths.to_owned(),
            };
        }
    }
    Impact {
        suites,
        paths: paths.to_owned(),
    }
}

pub fn impact(base: Option<&str>, paths: Vec<String>) -> Result<(), Box<dyn Error>> {
    let paths = if let Some(base) = base {
        let output = Command::new("git")
            .args(["diff", "--name-only", "-z", base, "HEAD"])
            .output()?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).to_string().into());
        }
        output
            .stdout
            .split(|byte| *byte == 0)
            .filter(|name| !name.is_empty())
            .map(|name| String::from_utf8_lossy(name).to_string())
            .collect()
    } else {
        paths
    };
    println!("{}", serde_json::to_string_pretty(&classify(&paths))?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_and_shared_paths_select_relevant_suites() {
        assert_eq!(
            classify(&["docs/testing.md".into()]).suites,
            BTreeSet::from(["static".into()])
        );
        assert!(
            classify(&["crates/protocol/src/lib.rs".into()])
                .suites
                .contains("performance")
        );
        assert_eq!(classify(&["Cargo.lock".into()]).suites.len(), 6);
        assert_eq!(
            classify(&["crates/assets/src/slp.rs".into()]).suites,
            BTreeSet::from(["static".into(), "native".into(), "assets".into()])
        );
        assert_eq!(
            classify(&["crates/client/src/lib.rs".into()]).suites,
            BTreeSet::from(["static".into(), "native".into(), "browser".into()])
        );
    }

    #[test]
    fn invalid_registry_is_rejected() {
        assert!(parse(br#"{"version":1,"gates":[{"id":"x","command":"make x","requires":["missing"],"select":"all","evidence":"test"}]}"#).is_err());
        for payload in [
            r#"{"version":2,"gates":[]}"#,
            r#"{"version":1,"gates":[{"id":"x","command":"make x","requires":[],"select":"all","evidence":"test"},{"id":"x","command":"make x","requires":[],"select":"all","evidence":"test"}]}"#,
            r#"{"version":1,"gates":[{"id":"x","command":"cargo test","requires":[],"select":"all","evidence":"test"}]}"#,
            r#"{"version":1,"gates":[{"id":"x","command":"make y","requires":[],"select":"all","evidence":"test"}]}"#,
        ] {
            assert!(parse(payload.as_bytes()).is_err(), "{payload}");
        }
    }

    #[test]
    fn documentation_gate_rejects_drift_and_missing_local_links() {
        let temp = tempfile::tempdir().expect("directory");
        let root = temp.path();
        fs::create_dir_all(root.join("gates")).expect("gates");
        fs::create_dir_all(root.join("docs/nested")).expect("docs");
        fs::create_dir_all(root.join("crates/example")).expect("crates");
        fs::write(
            root.join("gates/registry.json"),
            r#"{"version":1,"gates":[{"id":"fmt-check","command":"make fmt-check","requires":[],"select":"all","evidence":"exit"}]}"#,
        )
        .expect("registry");
        let registry =
            parse(&fs::read(root.join("gates/registry.json")).expect("registry")).expect("parsed");
        fs::write(root.join("docs/gates.md"), table(&registry)).expect("generated docs");
        fs::write(
            root.join("docs/nested/guide.md"),
            "[gates](../gates.md) [web](https://example.com) [heading](#top)",
        )
        .expect("guide");
        docs_check(root).expect("valid docs");
        fs::write(root.join("docs/nested/guide.md"), "[missing](gone.md)").expect("broken link");
        assert!(docs_check(root).is_err());
        fs::write(root.join("docs/nested/guide.md"), "[gates](../gates.md)")
            .expect("repaired link");
        fs::write(root.join("docs/gates.md"), "stale").expect("drifted docs");
        assert!(docs_check(root).is_err());
    }

    #[test]
    fn impact_accepts_git_base_and_rejects_unknown_ref() {
        impact(Some("HEAD"), Vec::new()).expect("known base");
        assert!(impact(Some("this-ref-does-not-exist"), Vec::new()).is_err());
    }
}
