//! The test-first report (docs/shipping.md#test-first), read from Git alone:
//! how the commits of a product-code pull request order tests and product
//! code, how many test functions came before the first product commit,
//! which build-time files changed, and any `Harness-Test-First: exempt —
//! <reason>` trailer. It informs the reviewers and the description; it never
//! refuses a ship.
#[cfg(test)]
mod tests;

use crate::ship::git;
use std::path::Path;

const TRAILER: &str = "Harness-Test-First";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// Changes tests of the product crates and no product code.
    TestsOnly,
    /// Changes product code and no test.
    Product,
    Mixed,
    /// Changes neither (harness, gates, docs, CI…).
    Other,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Self::TestsOnly => "tests-only",
            Self::Product => "product",
            Self::Mixed => "mixed",
            Self::Other => "other",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Report {
    /// Commits oldest first, by full id.
    pub(crate) commits: Vec<(String, Kind)>,
    /// `#[test]`-style functions added before the first product commit.
    pub(crate) tests_before: usize,
    pub(crate) build: Vec<String>,
    /// The commit and reason of the latest exemption trailer.
    pub(crate) exemption: Option<(String, String)>,
}

/// Under the product crates: `crates/` except `crates/harness/`.
fn in_scope(path: &str) -> bool {
    path.starts_with("crates/") && !path.starts_with("crates/harness/")
}

fn is_test(path: &str) -> bool {
    crate::agents::is_test_path(&path.to_ascii_lowercase())
}

/// An attribute line marking a test function: `#[test]`, `#[tokio::test]`,
/// `#[rstest]`, `#[wasm_bindgen_test]`; never a `cfg`.
pub(crate) fn test_marker(line: &str) -> bool {
    let Some(rest) = line.trim().strip_prefix("#[") else {
        return false;
    };
    let path: String = rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == ':')
        .collect();
    let last = path.rsplit("::").next().unwrap_or_default();
    last.contains("test") && !last.starts_with("cfg")
}

/// The reason in an `exempt — <reason>` trailer value.
pub(crate) fn exemption(value: &str) -> Option<String> {
    let rest = value.trim().strip_prefix("exempt")?;
    let reason = rest.trim_start_matches([' ', '—', '–', '-', ':']).trim();
    (rest.starts_with([' ', '—', '–', '-', ':']) && !reason.is_empty()).then(|| reason.to_owned())
}

/// Build scripts by name, Cargo and nextest configuration, the toolchain.
pub(crate) fn build_path(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    name == "build.rs"
        || name.starts_with("rust-toolchain")
        || path.split('/').any(|part| part == ".cargo")
        || path == ".config/nextest.toml"
        || path.ends_with("/.config/nextest.toml")
}

/// A manifest's build-time keys, or its text when it does not parse.
pub(crate) fn build_keys(manifest: &str) -> Result<Vec<Option<toml::Value>>, String> {
    let parsed: toml::Table = manifest.parse().map_err(|_| manifest.to_owned())?;
    let package = parsed.get("package");
    let mut keys: Vec<Option<toml::Value>> = ["build", "links"]
        .iter()
        .map(|key| package.and_then(|p| p.get(*key)).cloned())
        .collect();
    keys.extend(
        ["build-dependencies", "build_dependencies", "profile"]
            .iter()
            .map(|key| parsed.get(*key).cloned()),
    );
    for table in parsed
        .get("target")
        .and_then(toml::Value::as_table)
        .into_iter()
    {
        keys.extend(table.values().map(|t| t.get("build-dependencies").cloned()));
    }
    Ok(keys)
}

fn show(root: &Path, revision: &str, path: &str) -> String {
    git::git(root, &["show", &format!("{revision}:{path}")]).unwrap_or_default()
}

/// Custom build scripts (`package.build = "path"`) of the manifests at `revision`.
fn custom_scripts(root: &Path, revision: &str) -> git::Result<Vec<String>> {
    let files = git::git(root, &["ls-tree", "-r", "--name-only", revision])?;
    let manifests = files
        .lines()
        .filter(|p| p.rsplit('/').next() == Some("Cargo.toml"));
    Ok(manifests
        .filter_map(|manifest| {
            let parsed: toml::Table = show(root, revision, manifest).parse().ok()?;
            let script = parsed.get("package")?.get("build")?.as_str()?.to_owned();
            let directory = manifest.strip_suffix("Cargo.toml").unwrap_or_default();
            Some(format!("{directory}{}", script.trim_start_matches("./")))
        })
        .collect())
}

fn build_files(root: &Path, base: &str, head: &str) -> git::Result<Vec<String>> {
    let mut scripts = custom_scripts(root, base)?;
    scripts.extend(custom_scripts(root, head)?);
    let changed = git::git(root, &["diff", "--name-only", "--no-renames", base, head])?;
    Ok(changed
        .lines()
        .filter(|path| {
            let manifest = path.rsplit('/').next() == Some("Cargo.toml");
            build_path(path)
                || scripts.iter().any(|script| script == path)
                || (manifest
                    && build_keys(&show(root, base, path)) != build_keys(&show(root, head, path)))
        })
        .map(str::to_owned)
        .collect())
}

fn classify(root: &Path, sha: &str) -> git::Result<Kind> {
    let paths = git::git(
        root,
        &[
            "diff-tree",
            "--no-commit-id",
            "--name-only",
            "-r",
            "--root",
            sha,
        ],
    )?;
    let scoped: Vec<&str> = paths.lines().filter(|p| in_scope(p)).collect();
    let tests = scoped.iter().any(|p| is_test(p));
    let product = scoped.iter().any(|p| !is_test(p));
    Ok(match (tests, product) {
        (true, false) => Kind::TestsOnly,
        (false, true) => Kind::Product,
        (true, true) => Kind::Mixed,
        (false, false) => Kind::Other,
    })
}

/// The report on `base..head`.
pub(crate) fn report(root: &Path, base: &str, head: &str) -> git::Result<Report> {
    let list = git::git(root, &["rev-list", "--reverse", &format!("{base}..{head}")])?;
    let mut report = Report {
        commits: Vec::new(),
        tests_before: 0,
        build: build_files(root, base, head)?,
        exemption: None,
    };
    for sha in list.lines() {
        report.commits.push((sha.to_owned(), classify(root, sha)?));
        let format = format!("--format=%(trailers:key={TRAILER},valueonly,unfold)");
        let values = git::git(root, &["show", "-s", &format, sha])?;
        if let Some(reason) = values.lines().find_map(exemption) {
            report.exemption = Some((sha.to_owned(), reason));
        }
    }
    let first = report
        .commits
        .iter()
        .position(|(_, kind)| matches!(kind, Kind::Product | Kind::Mixed));
    if let Some(first) = first.filter(|first| *first > 0) {
        let before = &report.commits[first - 1].0;
        let diff = git::git(
            root,
            &[
                "diff",
                base,
                before,
                "--",
                "crates/",
                ":(exclude)crates/harness/",
            ],
        )?;
        report.tests_before = diff
            .lines()
            .filter(|l| !l.starts_with("+++"))
            .filter_map(|l| l.strip_prefix('+'))
            .filter(|l| test_marker(l))
            .count();
    }
    Ok(report)
}

impl Report {
    pub(crate) fn applicable(&self) -> bool {
        self.commits
            .iter()
            .any(|(_, kind)| matches!(kind, Kind::Product | Kind::Mixed))
    }

    /// Markdown bullet lines, for the review facts and the description.
    pub(crate) fn markdown(&self) -> String {
        if !self.applicable() {
            return "- test-first: not applicable (no product code changed; harness, gates, docs and CI are out of scope)\n".to_owned();
        }
        let short = |sha: &str| sha[..sha.len().min(12)].to_owned();
        let commits: Vec<String> = self
            .commits
            .iter()
            .map(|(sha, kind)| format!("`{}` {}", short(sha), kind.name()))
            .collect();
        let build = if self.build.is_empty() {
            "none".to_owned()
        } else {
            self.build.join(", ")
        };
        let exemption = self
            .exemption
            .as_ref()
            .map_or("none".to_owned(), |(sha, why)| {
                format!("`{}`: {why}", short(sha))
            });
        format!(
            "- test-first commits, oldest first: {}\n- `#[test]` functions added before the first product commit: {}\n- build-time files changed: {build}\n- test-first exemption: {exemption}\n",
            commits.join(", "),
            self.tests_before,
        )
    }
}

/// The report as Markdown; an unreadable history is reported, never fatal.
pub(crate) fn summary(root: &Path, base: &str, head: &str) -> String {
    report(root, base, head).map_or_else(
        |error| format!("- test-first report unavailable: {error}\n"),
        |report| report.markdown(),
    )
}
