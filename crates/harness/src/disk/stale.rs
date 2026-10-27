//! `make clean-stale`: classify every worktree of the repository and, with `--apply`,
//! remove merged clean worktrees and the `target/` of idle ones.
use super::precheck::GIB;
use crate::ship::git::git;
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const BASE: &str = "origin/dev";
/// A worktree untouched this long is idle; a merged one touched more recently is kept,
/// so a worktree just created from `origin/dev` is not mistaken for a finished one.
pub(crate) const IDLE: Duration = Duration::from_secs(3 * 24 * 3600);
pub(crate) const GRACE: Duration = Duration::from_secs(3600);
const HINT_BELOW: u64 = 50 * GIB;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(crate) enum Class {
    Merged,
    Idle,
    Active,
}

pub(crate) struct Row {
    pub(crate) path: PathBuf,
    pub(crate) branch: String,
    pub(crate) class: Class,
    pub(crate) outcome: String,
    pub(crate) freed: u64,
    target: u64,
    modified: SystemTime,
}

pub(crate) fn run(apply: bool) -> Result<()> {
    let now = SystemTime::now();
    let rows = clean(Path::new("."), apply, now, &|root, branch, tip| {
        merged_pull_request(root, branch, tip)
    })?;
    println!("{}", table(&rows, now));
    let freed: u64 = rows.iter().map(|row| row.freed).sum();
    let mode = if apply {
        "freed"
    } else {
        "dry run; APPLY=1 would free about"
    };
    println!(
        "{mode} {freed} bytes ({:.2} GiB)",
        freed as f64 / GIB as f64
    );
    Ok(())
}

/// The line `make ship` ends with when the disk runs low.
pub(crate) fn hint(free: u64) -> Option<String> {
    (free < HINT_BELOW).then(|| {
        format!(
            "disk: {} GiB free; run `make clean-stale` (dry run) / `make clean-stale APPLY=1`",
            free / GIB
        )
    })
}

pub(crate) fn clean(
    cwd: &Path,
    apply: bool,
    now: SystemTime,
    merged_pr: &dyn Fn(&Path, &str, &str) -> bool,
) -> Result<Vec<Row>> {
    let current = fs::canonicalize(git(cwd, &["rev-parse", "--show-toplevel"])?)?;
    let listing = crate::ship::git::git_raw(cwd, &["worktree", "list", "--porcelain", "-z"])?;
    let records = listing.split("\0\0").filter(|r| !r.is_empty());
    let mut main = None;
    let mut rows = Vec::new();
    for record in records {
        let field = |key: &str| record.split('\0').find_map(|f| f.strip_prefix(key));
        let (Some(path), Some(tip)) = (field("worktree "), field("HEAD ")) else {
            continue; // bare repository
        };
        let path = PathBuf::from(path);
        let main = main.get_or_insert_with(|| path.clone());
        if !path.is_dir() {
            continue; // prunable: the directory is already gone
        }
        let branch = field("branch refs/heads/")
            .unwrap_or("(detached)")
            .to_owned();
        let (total, target, modified) = scan(&path)?;
        let merged = git(main, &["merge-base", "--is-ancestor", tip, BASE]).is_ok()
            || (branch != "(detached)" && merged_pr(main, &branch, tip));
        let age = now.duration_since(modified).unwrap_or_default();
        let class = match () {
            () if merged => Class::Merged,
            () if age >= IDLE => Class::Idle,
            () => Class::Active,
        };
        let kept = if path == *main {
            Some("kept: main checkout")
        } else if fs::canonicalize(&path)? == current {
            Some("kept: current worktree")
        } else if class == Class::Active {
            Some("kept: active")
        } else if class == Class::Merged && age < GRACE {
            Some("kept: modified in the last hour")
        } else if git(&path, &["status", "--porcelain"]).map_or(true, |s| !s.is_empty()) {
            Some("kept: uncommitted changes")
        } else if in_use(&path) {
            Some("kept: in use")
        } else {
            None
        };
        let (outcome, freed) = match (kept, class, apply) {
            (Some(kept), ..) => (kept.to_owned(), 0),
            (None, Class::Merged, false) => ("would remove worktree".into(), total),
            (None, _, false) => ("would remove target/".into(), target),
            (None, Class::Merged, true) => {
                match git(main, &["worktree", "remove", path_str(&path)?]) {
                    Ok(_) => ("removed worktree".into(), total),
                    Err(error) => (format!("error: {error}"), 0),
                }
            }
            (None, _, true) => match super::remove(&path, Path::new("target")) {
                Ok(bytes) => ("removed target/".into(), bytes),
                Err(error) => (format!("error: {error}"), 0),
            },
        };
        let row = Row {
            path,
            branch,
            class,
            outcome,
            freed,
            target,
            modified,
        };
        rows.push(row);
    }
    Ok(rows)
}

fn path_str(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| format!("{} is not UTF-8", path.display()).into())
}

/// Total bytes, `target/` bytes and the newest modification time, without following symlinks.
fn scan(root: &Path) -> Result<(u64, u64, SystemTime)> {
    let mut sizes = (0u64, 0u64, SystemTime::UNIX_EPOCH);
    let mut pending = vec![(root.to_owned(), false)];
    // Unreadable entries (root-owned container output) are skipped, not fatal.
    while let Some((path, in_target)) = pending.pop() {
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            continue;
        };
        sizes.2 = sizes.2.max(metadata.modified()?);
        if metadata.is_dir() {
            let Ok(entries) = fs::read_dir(&path) else {
                continue;
            };
            for entry in entries.flatten() {
                let target = in_target || (path == root && entry.file_name() == "target");
                pending.push((entry.path(), target));
            }
        } else if metadata.is_file() {
            sizes.0 += metadata.len();
            sizes.1 += if in_target { metadata.len() } else { 0 };
        }
    }
    Ok(sizes)
}

/// Whether any process this user can inspect has its working directory inside `path`.
pub(crate) fn in_use(path: &Path) -> bool {
    let Ok(path) = fs::canonicalize(path) else {
        return false;
    };
    fs::read_dir("/proc")
        .into_iter()
        .flatten()
        .flatten()
        .any(|entry| {
            fs::read_link(entry.path().join("cwd")).is_ok_and(|cwd| cwd.starts_with(&path))
        })
}

/// A squash-merged branch: a merged pull request whose head is exactly this tip.
fn merged_pull_request(root: &Path, branch: &str, tip: &str) -> bool {
    let args = [
        "pr",
        "list",
        "--state",
        "merged",
        "--head",
        branch,
        "--json",
        "number,headRefOid",
    ];
    std::process::Command::new("gh")
        .current_dir(root)
        .args(args)
        .output()
        .is_ok_and(|out| {
            out.status.success() && merged_head(&String::from_utf8_lossy(&out.stdout), tip)
        })
}

pub(crate) fn merged_head(json: &str, tip: &str) -> bool {
    serde_json::from_str::<Vec<serde_json::Value>>(json)
        .is_ok_and(|prs| prs.iter().any(|pr| pr["headRefOid"].as_str() == Some(tip)))
}

fn table(rows: &[Row], now: SystemTime) -> String {
    let mut out = format!(
        "{:<52} {:<40} {:<7} {:>9} {:>9}  outcome\n",
        "path", "branch", "class", "target", "modified"
    );
    for row in rows {
        let days = now
            .duration_since(row.modified)
            .unwrap_or_default()
            .as_secs_f64()
            / 86400.0;
        out.push_str(&format!(
            "{:<52} {:<40} {:<7} {:>5.1} GiB {:>6.1}d ago  {}\n",
            row.path.display(),
            row.branch,
            format!("{:?}", row.class).to_uppercase(),
            row.target as f64 / GIB as f64,
            days,
            row.outcome
        ));
    }
    out
}
