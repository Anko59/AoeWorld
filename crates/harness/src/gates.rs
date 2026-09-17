//! Single gate registry, documentation rendering, and path-based impact selection.
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fs::{self, OpenOptions},
    io::Write,
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
    if paths.is_empty() {
        return Impact {
            suites: all,
            paths: Vec::new(),
        };
    }
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
            suites.insert("performance".to_owned());
        } else if path.starts_with("crates/assets/") {
            suites.insert("assets".to_owned());
            suites.insert("native".to_owned());
            suites.insert("browser".to_owned());
            suites.insert("performance".to_owned());
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

const CI_JOBS: [&str; 5] = [
    "static",
    "native-coverage",
    "browser",
    "target-performance",
    "fuzz-smoke",
];

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq)]
struct Selection {
    version: u16,
    revision: String,
    base: Option<String>,
    paths: Vec<String>,
    jobs: BTreeMap<String, bool>,
}

fn git_output(args: &[&str]) -> Result<String, Box<dyn Error>> {
    let output = Command::new("git").args(args).output()?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string().into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn changed_paths(base: &str) -> Result<Vec<String>, Box<dyn Error>> {
    let output = Command::new("git")
        .args(["diff", "--name-only", "-z", base, "HEAD"])
        .output()?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string().into());
    }
    Ok(output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|name| !name.is_empty())
        .map(|name| String::from_utf8_lossy(name).to_string())
        .collect())
}

fn selection(revision: String, base: Option<String>, paths: Vec<String>) -> Selection {
    let suites = classify(&paths).suites;
    let jobs = CI_JOBS
        .into_iter()
        .map(|job| {
            let selected = match job {
                "static" => true,
                "native-coverage" => suites.contains("native"),
                "browser" => suites.contains("browser"),
                "target-performance" => suites.contains("performance"),
                "fuzz-smoke" => suites.contains("assets") || suites.contains("release"),
                _ => false,
            };
            (job.to_owned(), selected)
        })
        .collect();
    Selection {
        version: 1,
        revision,
        base,
        paths,
        jobs,
    }
}

fn current_selection(base: Option<&str>) -> Result<Selection, Box<dyn Error>> {
    let revision = git_output(&["rev-parse", "HEAD"])?;
    let paths = match base {
        Some(base) => changed_paths(base)?,
        None => Vec::new(),
    };
    Ok(selection(revision, base.map(str::to_owned), paths))
}

pub fn ci_select() -> Result<(), Box<dyn Error>> {
    let base = std::env::var("AOE_BASE_SHA")
        .ok()
        .filter(|value| !value.is_empty());
    let selection = current_selection(base.as_deref())?;
    let json = serde_json::to_string(&selection)?;
    fs::create_dir_all("reports/gates")?;
    fs::write("reports/gates/selection.json", format!("{json}\n"))?;
    if let Some(path) = std::env::var_os("GITHUB_OUTPUT") {
        let mut output = OpenOptions::new().append(true).open(path)?;
        writeln!(output, "manifest={json}")?;
        for (job, selected) in &selection.jobs {
            writeln!(output, "{}={selected}", job.replace('-', "_"))?;
        }
    }
    println!("{json}");
    Ok(())
}

fn check_selection(
    manifest: &str,
    results: &str,
    expected: &Selection,
) -> Result<(), Box<dyn Error>> {
    let actual: Selection = serde_json::from_str(manifest)?;
    if &actual != expected {
        return Err(
            "CI selection manifest differs from the checked-out revision and changed paths".into(),
        );
    }
    let results: BTreeMap<String, String> = serde_json::from_str(results)?;
    if results.get("select").map(String::as_str) != Some("success")
        || results.len() != CI_JOBS.len() + 1
    {
        return Err("CI selection job failed or result set is incomplete".into());
    }
    for job in CI_JOBS {
        let wanted = if *expected.jobs.get(job).ok_or("missing selected job")? {
            "success"
        } else {
            "skipped"
        };
        if results.get(job).map(String::as_str) != Some(wanted) {
            return Err(format!("CI job {job} must be {wanted}").into());
        }
    }
    Ok(())
}

pub fn ci_check() -> Result<(), Box<dyn Error>> {
    let manifest = std::env::var("AOE_SELECTION_JSON")?;
    let results = std::env::var("AOE_JOB_RESULTS_JSON")?;
    let base = std::env::var("AOE_BASE_SHA")
        .ok()
        .filter(|value| !value.is_empty());
    let expected = current_selection(base.as_deref())?;
    check_selection(&manifest, &results, &expected)?;
    println!(
        "CI selection and all selected jobs passed for {}",
        expected.revision
    );
    Ok(())
}

pub fn impact(base: Option<&str>, paths: Vec<String>) -> Result<(), Box<dyn Error>> {
    let paths = if let Some(base) = base {
        changed_paths(base)?
    } else {
        paths
    };
    println!("{}", serde_json::to_string_pretty(&classify(&paths))?);
    Ok(())
}

#[cfg(test)]
mod tests;
