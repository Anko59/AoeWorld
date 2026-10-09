//! The test-first report (docs/shipping.md#test-first), read from Git alone:
//! how the commits of a product-code pull request order tests and product
//! code, how many test functions came before the first product commit,
//! which product files carry inline `#[cfg(test)]` changes, which
//! build-time files changed, and any `Harness-Test-First: exempt —
//! <reason>` trailer. It informs the reviewers and the description; it never
//! refuses a ship.
mod inline;
#[cfg(test)]
mod inline_tests;
#[cfg(test)]
mod tests;

use crate::ship::git;
use std::{collections::BTreeSet, path::Path};

const TRAILER: &str = "Harness-Test-First";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// Changes tests of the product crates, in test files or inside
    /// `#[cfg(test)]` items, and no product code.
    TestsOnly,
    /// Changes product code and no test.
    Product,
    Mixed,
    /// Changes neither (harness, gates, docs, CI…).
    Other,
    /// A merge, judged by its diff against its first parent.
    Merge(Base),
}

/// The kind of a merge's first-parent diff.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Base {
    TestsOnly,
    Product,
    Mixed,
    Other,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Self::TestsOnly => "tests-only",
            Self::Product => "product",
            Self::Mixed => "mixed",
            Self::Other => "other",
            Self::Merge(Base::TestsOnly) => "merge (tests-only)",
            Self::Merge(Base::Product) => "merge (product)",
            Self::Merge(Base::Mixed) => "merge (mixed)",
            Self::Merge(Base::Other) => "merge (other)",
        }
    }

    fn product(self) -> bool {
        matches!(
            self,
            Self::Product | Self::Mixed | Self::Merge(Base::Product | Base::Mixed)
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Report {
    /// Commits oldest first, by full id.
    pub(crate) commits: Vec<(String, Kind)>,
    /// `#[test]`-style functions added before the first product commit.
    pub(crate) tests_before: usize,
    /// Product files whose `#[cfg(test)]` items changed.
    pub(crate) inline: Vec<String>,
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
    // Each target's build dependencies under its selector, so moving a table
    // to another target is a change.
    let targets: toml::Table = parsed
        .get("target")
        .and_then(toml::Value::as_table)
        .into_iter()
        .flatten()
        .filter_map(|(selector, table)| {
            let deps: toml::Table = ["build-dependencies", "build_dependencies"]
                .iter()
                .filter_map(|key| Some(((*key).to_owned(), table.get(*key)?.clone())))
                .collect();
            (!deps.is_empty()).then(|| (selector.clone(), toml::Value::Table(deps)))
        })
        .collect();
    keys.push(Some(toml::Value::Table(targets)));
    Ok(keys)
}

/// Every build-dependency table of a manifest, target-specific ones included.
fn build_tables(parsed: &toml::Table) -> Vec<&toml::Table> {
    let targets = parsed
        .get("target")
        .and_then(toml::Value::as_table)
        .into_iter()
        .flat_map(|table| table.values());
    std::iter::once(parsed)
        .chain(targets.filter_map(toml::Value::as_table))
        .flat_map(|table| {
            ["build-dependencies", "build_dependencies"]
                .iter()
                .filter_map(|key| table.get(*key)?.as_table())
        })
        .collect()
}

/// Build dependencies a manifest inherits with `name.workspace = true`.
pub(crate) fn inherited(parsed: &toml::Table) -> BTreeSet<String> {
    build_tables(parsed)
        .into_iter()
        .flatten()
        .filter(|(_, spec)| spec.get("workspace").and_then(toml::Value::as_bool) == Some(true))
        .map(|(name, _)| name.clone())
        .collect()
}

/// The `[workspace.dependencies]` entries named in `names`, or the text when
/// the manifest does not parse.
pub(crate) fn workspace_keys(
    manifest: &str,
    names: &BTreeSet<String>,
) -> Result<Vec<Option<toml::Value>>, String> {
    let parsed: toml::Table = manifest.parse().map_err(|_| manifest.to_owned())?;
    let table = parsed.get("workspace").and_then(|w| w.get("dependencies"));
    Ok(names
        .iter()
        .map(|name| table.and_then(|t| t.get(name)).cloned())
        .collect())
}

fn show(root: &Path, revision: &str, path: &str) -> String {
    git::git(root, &["show", &format!("{revision}:{path}")]).unwrap_or_default()
}

/// The manifests that parse at `revision`, by path.
fn manifests(root: &Path, revision: &str) -> git::Result<Vec<(String, toml::Table)>> {
    let files = git::git(root, &["ls-tree", "-r", "--name-only", revision])?;
    Ok(files
        .lines()
        .filter(|p| p.rsplit('/').next() == Some("Cargo.toml"))
        .filter_map(|path| Some((path.to_owned(), show(root, revision, path).parse().ok()?)))
        .collect())
}

/// The custom build script (`package.build = "path"`) of a manifest.
fn custom_script((manifest, parsed): &(String, toml::Table)) -> Option<String> {
    let script = parsed.get("package")?.get("build")?.as_str()?;
    let directory = manifest.strip_suffix("Cargo.toml").unwrap_or_default();
    Some(format!("{directory}{}", script.trim_start_matches("./")))
}

fn build_files(root: &Path, base: &str, head: &str) -> git::Result<Vec<String>> {
    let mut all = manifests(root, base)?;
    all.extend(manifests(root, head)?);
    let scripts: Vec<String> = all.iter().filter_map(custom_script).collect();
    let names: BTreeSet<String> = all.iter().flat_map(|(_, m)| inherited(m)).collect();
    let changed = git::git(root, &["diff", "--name-only", "--no-renames", base, head])?;
    Ok(changed
        .lines()
        .filter(|path| {
            let manifest = path.rsplit('/').next() == Some("Cargo.toml");
            build_path(path)
                || scripts.iter().any(|script| script == path)
                || (manifest
                    && (build_keys(&show(root, base, path)) != build_keys(&show(root, head, path))
                        || workspace_keys(&show(root, base, path), &names)
                            != workspace_keys(&show(root, head, path), &names)))
        })
        .map(str::to_owned)
        .collect())
}

/// The kind of `sha` against `parent` (none for a root commit), and the
/// product files whose `#[cfg(test)]` items it changed.
fn classify(root: &Path, sha: &str, parent: Option<&str>) -> git::Result<(Base, Vec<String>)> {
    let mut args = vec![
        "diff-tree",
        "--no-commit-id",
        "--no-renames",
        "-r",
        "-p",
        "-U0",
    ];
    match parent {
        Some(parent) => args.push(parent),
        None => args.push("--root"),
    }
    args.extend([sha, "--", "crates/", ":(exclude)crates/harness/"]);
    let patch = git::git(root, &args)?;
    let (mut tests, mut product, mut inline) = (false, false, Vec::new());
    for (path, changes) in inline::changes(&patch) {
        if !in_scope(&path) {
            continue;
        }
        if is_test(&path) {
            tests = true;
            continue;
        }
        let (test_lines, product_lines) = if path.ends_with(".rs") {
            let before = parent.map_or_else(String::new, |p| show(root, p, &path));
            inline::split(&changes, &before, &show(root, sha, &path))
        } else {
            (0, 0)
        };
        if test_lines > 0 {
            inline.push(path);
        }
        // A binary or mode-only change has no lines and stays product code.
        if test_lines > 0 && product_lines == 0 {
            tests = true;
        } else {
            product = true;
        }
    }
    let base = match (tests, product) {
        (true, false) => Base::TestsOnly,
        (false, true) => Base::Product,
        (true, true) => Base::Mixed,
        (false, false) => Base::Other,
    };
    Ok((base, inline))
}

/// The report on `base..head`.
pub(crate) fn report(root: &Path, base: &str, head: &str) -> git::Result<Report> {
    let range = format!("{base}..{head}");
    let list = git::git(root, &["rev-list", "--reverse", "--parents", &range])?;
    let mut report = Report {
        commits: Vec::new(),
        tests_before: 0,
        inline: Vec::new(),
        build: build_files(root, base, head)?,
        exemption: None,
    };
    for line in list.lines() {
        let mut ids = line.split(' ');
        let sha = ids.next().unwrap_or_default();
        let parents: Vec<&str> = ids.collect();
        let (kind, inline) = classify(root, sha, parents.first().copied())?;
        let kind = match (parents.len() > 1, kind) {
            (true, kind) => Kind::Merge(kind),
            (false, Base::TestsOnly) => Kind::TestsOnly,
            (false, Base::Product) => Kind::Product,
            (false, Base::Mixed) => Kind::Mixed,
            (false, Base::Other) => Kind::Other,
        };
        report.commits.push((sha.to_owned(), kind));
        for path in inline {
            if !report.inline.contains(&path) {
                report.inline.push(path);
            }
        }
        let format = format!("--format=%(trailers:key={TRAILER},valueonly,unfold)");
        let values = git::git(root, &["show", "-s", &format, sha])?;
        if let Some(reason) = values.lines().find_map(exemption) {
            report.exemption = Some((sha.to_owned(), reason));
        }
    }
    let first = report.commits.iter().position(|(_, kind)| kind.product());
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
    fn has_product(&self) -> bool {
        self.commits.iter().any(|(_, kind)| kind.product())
    }

    /// Product code or a build-time file changed.
    pub(crate) fn applicable(&self) -> bool {
        self.has_product() || !self.build.is_empty()
    }

    /// Markdown bullet lines, for the review facts and the description.
    pub(crate) fn markdown(&self) -> String {
        if !self.applicable() {
            return "- test-first: not applicable (no product code or build-time file changed; harness, gates, docs and CI are out of scope)\n".to_owned();
        }
        let short = |sha: &str| sha[..sha.len().min(12)].to_owned();
        let commits: Vec<String> = self
            .commits
            .iter()
            .map(|(sha, kind)| format!("`{}` {}", short(sha), kind.name()))
            .collect();
        let list = |paths: &[String]| {
            if paths.is_empty() {
                "none".to_owned()
            } else {
                paths.join(", ")
            }
        };
        let tests_before = if self.has_product() {
            self.tests_before.to_string()
        } else {
            "none (no product commit)".to_owned()
        };
        let exemption = self
            .exemption
            .as_ref()
            .map_or("none".to_owned(), |(sha, why)| {
                format!("`{}`: {why}", short(sha))
            });
        format!(
            "- test-first commits, oldest first: {}\n- `#[test]` functions added before the first product commit: {tests_before}\n- inline `#[cfg(test)]` changes in product files (the rule is tests in test files): {}\n- build-time files changed: {}\n- test-first exemption: {exemption}\n",
            commits.join(", "),
            list(&self.inline),
            list(&self.build),
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
