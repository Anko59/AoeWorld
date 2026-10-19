use super::{Record, blobs, changed_tests, git};
use std::{
    fs,
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
};

// Index by patch first, then bound inspection even for repeated proofs of one patch.
const CANDIDATE_LIMIT: usize = 128;

fn full_id(head: &str) -> bool {
    matches!(head.len(), 40 | 64) && head.bytes().all(|b| b.is_ascii_hexdigit())
}

fn directory(root: &Path) -> git::Result<PathBuf> {
    let directory = crate::ship::evidence::directory(root)?;
    Ok(directory.parent().ok_or("no ship directory")?.join("red"))
}

fn path(root: &Path, head: &str) -> git::Result<PathBuf> {
    if !full_id(head) {
        return Err("red record requires a full commit id".into());
    }
    Ok(directory(root)?.join(format!("{head}.json")))
}

fn index(root: &Path, fingerprint: &str) -> git::Result<PathBuf> {
    if fingerprint.len() != 64 || !fingerprint.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("red record requires a patch fingerprint".into());
    }
    Ok(directory(root)?.join("by-fingerprint").join(fingerprint))
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
    let index = index(root, &record.change_fingerprint)?;
    fs::create_dir_all(&index).map_err(|e| e.to_string())?;
    // Only the filename matters. Readers load and validate the authoritative head record;
    // a stale index entry (or an interrupted write before indexing) never grants a PASS.
    fs::write(index.join(format!("{}.json", record.head)), b"").map_err(|e| e.to_string())?;
    Ok(())
}

fn valid(record: &Record, paths: &[String], blobs: &[String]) -> bool {
    record.paths == paths
        && record.blobs == blobs
        && !(record.failed.is_empty() && record.missing.is_empty())
        && (record.missing.is_empty() || !record.ran.is_empty())
        && !record
            .failed
            .iter()
            .any(|name| record.base_failed.contains(name))
}

fn carried(
    root: &Path,
    fingerprint: &str,
    paths: &[String],
    blobs: &[String],
) -> git::Result<Option<Record>> {
    let entries = match fs::read_dir(index(root, fingerprint)?) {
        Ok(entries) => entries,
        Err(_) => return Ok(None),
    };
    for entry in entries.take(CANDIDATE_LIMIT).flatten() {
        let name = entry.file_name();
        let Some(head) = name
            .to_str()
            .and_then(|s| s.strip_suffix(".json"))
            .filter(|s| full_id(s))
        else {
            continue;
        };
        let Some(record) = fs::read(path(root, head)?)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Record>(&bytes).ok())
        else {
            continue;
        };
        if record.head == head
            && full_id(&record.base)
            && record.change_fingerprint == fingerprint
            && valid(&record, paths, blobs)
            // Like review reuse, recompute the source against its recorded base, not a ref
            // that may have moved. The index and stored hash are only lookup hints.
            && git::change_fingerprint_from(root, &record.base, head)
                .is_ok_and(|(_, source)| source == fingerprint)
        {
            return Ok(Some(record));
        }
    }
    Ok(None)
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
    let paths = changed_tests(root, base, head)?;
    let blobs = blobs(root, head, &paths)?;
    let fingerprint = git::change_fingerprint_from(root, base, head)?.1;
    let record: Record = match fs::read(path(root, head)?) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|_| missing())?,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            let source = carried(root, &fingerprint, &paths, &blobs)?.ok_or_else(missing)?;
            return Ok(format!(
                "- test-first red: PASS (carried from {}; patch unchanged after rebase)\n",
                source.head
            ));
        }
        Err(_) => return Err(missing()),
    };
    if record.head != head
        || record.base != base
        || record.change_fingerprint != fingerprint
        || !valid(&record, &paths, &blobs)
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
