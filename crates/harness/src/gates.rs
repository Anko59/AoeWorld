//! Single gate registry, documentation rendering, and path-based impact selection.
pub(crate) mod dispatch_status;
pub(crate) mod flaky;
mod paths;
pub(crate) mod registry;
mod resource_limits;
pub(crate) mod review;
pub(crate) mod runner;
pub(crate) mod scopes;
pub(crate) mod select;
pub(crate) mod worker;
use registry::{Cadence, Registry};
use select::{Graph, Rules, UnitScope};

use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

#[cfg(test)]
fn parse(bytes: &[u8]) -> Result<Registry, Box<dyn Error>> {
    Registry::parse(bytes)
}

fn table(registry: &Registry) -> String {
    let mut output = "# Implemented gate registry\n\nGenerated from `gates/registry.json`. `make docs-check` detects drift.\n\nRegistry v2 drives selection and dependency plans. The opt-in gate-run CLI executes cadence or complete CI-job plans with explicit budgets and local evidence. Existing Make/CI dispatch and minimum preflight remain mandatory until protected judging replaces bootstrap execution. No automatic agent interception is implied.\n\n".to_owned();
    output.push_str(&resource_limits::paragraph());
    output.push_str("| Gate | Command | Depends on | Suites | Cadences | Budget (s) | Long job | Evidence |\n|---|---|---|---|---|---|---|---|\n");
    for gate in &registry.gates {
        let requires = if gate.requires.is_empty() {
            "—".to_owned()
        } else {
            gate.requires.join(", ")
        };
        output.push_str(&format!(
            "| {} | `{}` | {} | {} | {:?} | {} | {} | {} |\n",
            gate.id,
            gate.command,
            requires,
            gate.suites.join(", "),
            gate.cadences,
            gate.budget_s,
            if gate.long { "yes" } else { "—" },
            gate.evidence
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
    flaky::Policy::load(root)?;
    let registry = Registry::load(root)?;
    Rules::load(root, &registry)?;
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
    pub reasons: BTreeMap<String, BTreeSet<String>>,
}

fn classify(registry: &Registry, paths: &[String]) -> Impact {
    let classification = registry.classify(paths);
    Impact {
        suites: classification.suites,
        paths: paths.to_vec(),
        reasons: classification.reasons,
    }
}

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct Selection {
    version: u16,
    revision: String,
    base: Option<String>,
    requested_base: Option<String>,
    paths: Vec<String>,
    registry_hash: String,
    gates: Vec<String>,
    jobs: BTreeMap<String, bool>,
    /// One sentence per job: "runs because ..." or "skipped because ...".
    reasons: BTreeMap<String, String>,
    /// The legs of the `static` job: `light` or `checks`, then `unit` when tests run.
    static_parts: Vec<String>,
    unit: UnitScope,
}

fn git_output(args: &[&str]) -> Result<String, Box<dyn Error>> {
    let output = Command::new("git").args(args).output()?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string().into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn repository_root() -> Result<PathBuf, Box<dyn Error>> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string().into());
    }
    let text = String::from_utf8(output.stdout)?;
    Ok(PathBuf::from(text.strip_suffix('\n').unwrap_or(&text)))
}

fn changed_paths(base: &str) -> Result<Vec<String>, Box<dyn Error>> {
    paths::changed(Path::new("."), base)
}

/// The gates the selected jobs execute: the members of each selected job that the cadence
/// runs, narrowed for the `static` job to the legs its manifest runs. A prerequisite that
/// no selected job runs is vacuous: its inputs did not change.
fn executed_gates(
    registry: &Registry,
    rules: &Rules,
    cadence: Cadence,
    decision: &select::Decision,
) -> Result<Vec<String>, Box<dyn Error>> {
    let leg = |gate: &str| match rules.part_of(gate) {
        Some("checks") => decision.static_parts.iter().any(|p| p == "checks"),
        Some("unit") => {
            decision.static_parts.iter().any(|p| p == "unit")
                && (gate != "perf-smoke" || decision.unit.perf_smoke)
        }
        _ => true,
    };
    let mut seeds = BTreeSet::new();
    for (job, members) in &registry.jobs {
        if decision.jobs.get(job) != Some(&true) {
            continue;
        }
        for id in members {
            let in_cadence = registry
                .gates
                .iter()
                .any(|g| &g.id == id && g.cadences.contains(&cadence));
            if in_cadence && (job != "static" || leg(id)) {
                seeds.insert(id.clone());
            }
        }
    }
    registry.dependency_order(&seeds)
}

fn selection(
    registry: &Registry,
    rules: &Rules,
    graph: &Graph,
    revision: String,
    base: Option<String>,
    paths: Vec<String>,
    pull_request: bool,
) -> Result<Selection, Box<dyn Error>> {
    // A pull request runs the `pr` cadence even when its base cannot be
    // resolved; a push to dev or main runs `ci`, which adds the slow gates
    // that need not block a merge.
    let cadence = if pull_request {
        Cadence::Pr
    } else {
        Cadence::Ci
    };
    let eligible = select::eligible(registry, cadence);
    let decision = select::decide(rules, graph, registry, &eligible, &paths);
    let gates = executed_gates(registry, rules, cadence, &decision)?;
    Ok(Selection {
        version: 3,
        revision,
        requested_base: base.clone(),
        base,
        paths,
        registry_hash: format!("{}+{}", registry.fingerprint()?, rules.fingerprint()?),
        gates,
        jobs: decision.jobs,
        reasons: decision.reasons,
        static_parts: decision.static_parts,
        unit: decision.unit,
    })
}

fn current_selection(base: Option<&str>) -> Result<Selection, Box<dyn Error>> {
    let root = repository_root()?;
    let registry = Registry::load(&root)?;
    let rules = Rules::load(&root, &registry)?;
    let revision = git_output(&["rev-parse", "HEAD"])?;
    let requested_base = base.map(str::to_owned);
    let (base, paths) = match base {
        Some(base) => match paths::resolve(Path::new("."), base)
            .and_then(|base| changed_paths(&base).map(|paths| (Some(base), paths)))
        {
            Ok(result) => result,
            Err(error) => {
                eprintln!("comparison unavailable; select all: {error}");
                (None, Vec::new())
            }
        },
        None => (None, Vec::new()),
    };
    let sources = paths.iter().any(|path| rules.reads(path));
    let graph = Graph::load(&root, &rules, sources)?;
    let mut manifest = selection(
        &registry,
        &rules,
        &graph,
        revision,
        base,
        paths,
        requested_base.is_some(),
    )?;
    manifest.requested_base = requested_base;
    Ok(manifest)
}

// A dispatcher chooses a ref, never the comparison that determines its gates:
// a dispatch compares with its merge base with dev, which stays the same when
// dev advances between the selection and the final check.
fn requested_ci_base() -> Option<String> {
    std::env::var("AOE_BASE_SHA")
        .ok()
        .filter(|value| !value.is_empty())
        .or_else(|| {
            (std::env::var("GITHUB_EVENT_NAME").as_deref() == Ok("workflow_dispatch")
                && std::env::var("GITHUB_REF_NAME")
                    .is_ok_and(|name| name != "dev" && name != "main"))
            .then(|| git_output(&["merge-base", "origin/dev", "HEAD"]).ok())
            .flatten()
        })
}

pub fn ci_select() -> Result<(), Box<dyn Error>> {
    let base = requested_ci_base();
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
        writeln!(
            output,
            "static_parts={}",
            serde_json::to_string(&selection.static_parts)?
        )?;
        writeln!(
            output,
            "unit_scope={}",
            serde_json::to_string(&selection.unit)?
        )?;
    }
    if let Some(path) = std::env::var_os("GITHUB_STEP_SUMMARY") {
        let mut summary = OpenOptions::new().append(true).open(path)?;
        write!(summary, "{}", explain(&selection, true))?;
    }
    println!("{json}");
    Ok(())
}

fn explain(selection: &Selection, markdown: bool) -> String {
    let render = if markdown {
        select::markdown
    } else {
        select::text
    };
    render(
        &selection.jobs,
        &selection.reasons,
        &selection.static_parts,
        &selection.unit,
    )
}

/// What CI will run for the current branch against `base`, job by job, and why.
pub fn ci_explain(base: &str) -> Result<(), Box<dyn Error>> {
    let selection = current_selection(Some(base))?;
    println!(
        "CI for {} against {} ({} changed paths); a push to dev or the nightly runs every job",
        selection.revision,
        selection.base.as_deref().unwrap_or("an unavailable base"),
        selection.paths.len()
    );
    if !git_output(&["status", "--porcelain"])?.is_empty() {
        println!("note: uncommitted changes are not counted; CI sees commits only");
    }
    print!("{}", explain(&selection, false));
    Ok(())
}

fn check_selection(
    manifest: &str,
    results: &str,
    expected: &Selection,
) -> Result<(), Box<dyn Error>> {
    // A failed or cancelled `select` leaves no manifest: say so instead of a JSON parse error.
    let job_results: BTreeMap<String, String> = serde_json::from_str(results)?;
    if let Some(select) = job_results
        .get("select")
        .filter(|r| r.as_str() != "success")
    {
        return Err(format!(
            "the select job ended `{select}`, so nothing was validated; re-run the failed jobs \
             (`gh run rerun <run> --failed`) and read the select log"
        )
        .into());
    }
    let actual: Selection = serde_json::from_str(manifest)?;
    if &actual != expected {
        return Err(
            "CI selection manifest differs from the checked-out revision and changed paths".into(),
        );
    }
    let results: BTreeMap<String, String> = serde_json::from_str(results)?;
    if results.get("select").map(String::as_str) != Some("success")
        || results.len() != expected.jobs.len() + 1
    {
        return Err("CI selection job failed or result set is incomplete".into());
    }
    for job in expected.jobs.keys() {
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
    let base = requested_ci_base();
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
    println!(
        "{}",
        serde_json::to_string_pretty(&classify(&Registry::load(&repository_root()?)?, &paths))?
    );
    Ok(())
}

pub fn docs_generate(root: &Path) -> Result<(), Box<dyn Error>> {
    fs::write(root.join("docs/gates.md"), table(&Registry::load(root)?))?;
    Ok(())
}

pub fn plan(
    cadence: Cadence,
    base: Option<&str>,
    input: Vec<String>,
) -> Result<(), Box<dyn Error>> {
    let paths = match base {
        Some(base) => changed_paths(base)?,
        None => input,
    };
    let registry = Registry::load(&repository_root()?)?;
    let classification = registry.classify(&paths);
    println!(
        "{}",
        serde_json::to_string_pretty(&registry.plan(cadence, &classification.suites)?)?
    );
    Ok(())
}

#[cfg(test)]
mod tests;
