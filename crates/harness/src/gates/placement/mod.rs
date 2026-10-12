//! Added inline tests, compared with origin/dev; existing tests are grandfathered.
mod syntax;
#[cfg(test)]
mod tests;

use crate::gates::scopes::{Kind, Snapshot};
use std::{
    collections::BTreeMap,
    error::Error,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn command(root: &Path) -> Command {
    let mut command = Command::new("git");
    command.current_dir(root);
    for (name, _) in std::env::vars_os() {
        if name.as_encoded_bytes().starts_with(b"GIT_") {
            command.env_remove(name);
        }
    }
    command
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .arg("--no-replace-objects");
    command
}

fn git(root: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let output = command(root).args(args).output()?;
    if !output.status.success() {
        return Err(format!("git {args:?}: {}", String::from_utf8_lossy(&output.stderr)).into());
    }
    Ok(output.stdout)
}

fn in_scope(path: &str) -> bool {
    path.starts_with("crates/")
        && path.ends_with(".rs")
        && !crate::agents::is_test_path(&path.to_ascii_lowercase())
}

/// Resolve history on the original source, before entering the exported index.
/// Exported scopes deliberately contain no remote refs or parent history.
pub(crate) struct Comparison {
    source: PathBuf,
    base: String,
    files: BTreeMap<String, String>,
}

impl Comparison {
    pub(crate) fn prepare(source: &Path, kind: &Kind) -> Result<Self> {
        let head = match kind {
            Kind::Commit(oid) => oid.as_str(),
            _ => "HEAD",
        };
        let base = git(source, &["merge-base", "refs/remotes/origin/dev", head])
            .map_err(|error| format!("test-placement comparison base unavailable: fetch origin/dev and full history (shallow clones cannot qualify): {error}"))?;
        let base = String::from_utf8(base)?.trim().to_owned();
        let listing = git(source, &["ls-tree", "-r", "-z", &base, "--", "crates/"])?;
        let mut files = BTreeMap::new();
        for entry in listing.split(|b| *b == 0).filter(|e| !e.is_empty()) {
            let entry = std::str::from_utf8(entry)?;
            let (header, path) = entry.split_once('\t').ok_or("invalid tree entry")?;
            if in_scope(path) {
                let oid = header.split_whitespace().nth(2).ok_or("missing blob ID")?;
                files.insert(path.to_owned(), oid.to_owned());
            }
        }
        Ok(Self {
            source: source.to_owned(),
            base,
            files,
        })
    }

    pub(crate) fn check(&self, root: &Path) -> Result<()> {
        let listing = git(
            root,
            &[
                "ls-files",
                "--cached",
                "--others",
                "--exclude-standard",
                "-z",
            ],
        )?;
        let mut failures = Vec::new();
        for path in listing.split(|b| *b == 0).filter(|p| !p.is_empty()) {
            let path = std::str::from_utf8(path)?;
            if !in_scope(path) {
                continue;
            }
            let file = root.join(path);
            if !file.exists() {
                continue;
            } // A deletion cannot gain tests.
            let after = fs::read_to_string(&file)?;
            let before = match self.files.get(path) {
                Some(oid) => String::from_utf8(git(&self.source, &["cat-file", "blob", oid])?)?,
                None => String::new(),
            };
            if before == after {
                continue;
            }
            let regions = match syntax::regions(&after) {
                Ok(regions) => regions,
                Err(error) => {
                    failures.push(format!(
                        "{path}:{}: unparsable Rust file: {error}",
                        error.span().start().line
                    ));
                    continue;
                }
            };
            if regions.is_empty() {
                continue;
            }
            let added = additions(root, &before, &file)?;
            for (start, end) in regions {
                if let Some(line) = added
                    .iter()
                    .copied()
                    .find(|line| (start..=end).contains(line))
                {
                    failures.push(format!("{path}:{line}: new inline test code; move the tests to <module>/tests.rs (or <name>_tests.rs) and declare `#[cfg(test)] mod tests;`"));
                }
            }
        }
        failures.sort();
        failures.dedup();
        if !failures.is_empty() {
            return Err(failures.join("\n").into());
        }
        println!("test-placement passed (merge base {})", self.base);
        Ok(())
    }
}

/// Git's line comparison, including blank/comment additions, with no filters.
fn additions(root: &Path, before: &str, after: &Path) -> Result<Vec<usize>> {
    let temporary = tempfile::tempdir()?;
    let old = temporary.path().join("before.rs");
    fs::write(&old, before)?;
    let output = command(root)
        .args([
            "diff",
            "--no-index",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            "--text",
            "--unified=0",
            "--diff-algorithm=myers",
            "--no-indent-heuristic",
            "--",
        ])
        .arg(&old)
        .arg(after)
        .output()?;
    if !matches!(output.status.code(), Some(0 | 1)) {
        return Err(format!(
            "test-placement line comparison failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let mut lines = Vec::new();
    for line in String::from_utf8(output.stdout)?.lines() {
        if let Some(hunk) = line.strip_prefix("@@ ") {
            let range = hunk
                .split_whitespace()
                .nth(1)
                .and_then(|r| r.strip_prefix('+'))
                .ok_or("invalid diff hunk")?;
            let (start, count) = range.split_once(',').unwrap_or((range, "1"));
            let start: usize = start.parse()?;
            let count: usize = count.parse()?;
            lines.extend(start..start + count);
        }
    }
    Ok(lines)
}

pub(crate) fn check(root: &Path, kind: Kind) -> Result<()> {
    let comparison = Comparison::prepare(root, &kind)?;
    let snapshot = Snapshot::prepare(root, kind)?;
    snapshot.run_checked(|root| comparison.check(root))
}
