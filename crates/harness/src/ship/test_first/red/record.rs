use super::{Record, blobs, changed_tests, git};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

fn path(root: &Path, head: &str) -> git::Result<PathBuf> {
    if !matches!(head.len(), 40 | 64) || !head.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("red record requires a full commit id".into());
    }
    let directory = crate::ship::evidence::directory(root)?;
    Ok(directory
        .parent()
        .ok_or("no ship directory")?
        .join("red")
        .join(format!("{head}.json")))
}

pub(super) fn write(root: &Path, record: &Record) -> git::Result<()> {
    let path = path(root, &record.head)?;
    let directory = path.parent().ok_or("red record has no directory")?;
    fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let mut file = tempfile::NamedTempFile::new_in(directory).map_err(|e| e.to_string())?;
    file.write_all(&serde_json::to_vec_pretty(record).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    file.as_file().sync_all().map_err(|e| e.to_string())?;
    file.persist(&path).map_err(|e| e.to_string())?;
    Ok(())
}

pub(crate) fn require(root: &Path, base: &str, head: &str) -> git::Result<String> {
    // Records are keyed by the full commit id; callers may pass a revision such as `HEAD`
    // (the review's deterministic facts do).
    let resolved = git::git(
        root,
        &["rev-parse", "--verify", &format!("{head}^{{commit}}")],
    )?;
    let head = resolved.as_str();
    let report = super::super::report(root, base, head)?;
    if let Some((commit, reason)) = &report.exemption {
        return Ok(format!(
            "- test-first red: exempt at {commit}: {reason} (reviewers must validate the exemption)\n"
        ));
    }
    if !report.applicable() {
        return Ok("- test-first red: not applicable (no product/build changes)\n".into());
    }
    let missing = || {
        format!(
            "ship: missing matching red record for {head}; run make red at this exact commit, or declare Harness-Test-First: exempt — <docs/config/pure refactor reason>"
        )
    };
    let bytes = fs::read(path(root, head)?).map_err(|_| missing())?;
    let record: Record = serde_json::from_slice(&bytes).map_err(|_| missing())?;
    let paths = changed_tests(root, base, head)?;
    if record.head != head
        || record.base != base
        || record.paths != paths
        || record.blobs != blobs(root, head, &paths)?
        || (record.failed.is_empty() && record.missing.is_empty())
        || (!record.missing.is_empty() && record.ran.is_empty())
        || record
            .failed
            .iter()
            .any(|name| record.base_failed.contains(name))
    {
        return Err(missing());
    }
    let evidence = if record.missing.is_empty() {
        format!("failed by name: {}", record.failed.join(", "))
    } else {
        format!(
            "failed to compile, missing: {}; ran and passed at head: {}{}",
            record.missing.join(", "),
            record.ran.join(", "),
            if record.failed.is_empty() {
                String::new()
            } else {
                format!("; failed by name: {}", record.failed.join(", "))
            }
        )
    };
    Ok(format!(
        "- test-first red: PASS for {head} (base {base}); the PR's tests ({}) were red on the merge base, green at head — {evidence}\n",
        record.paths.join(", ")
    ))
}
