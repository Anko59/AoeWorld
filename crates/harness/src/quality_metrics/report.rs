use super::Result;
use serde_json::Value;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

pub(super) fn write_history(
    directory: &Path,
    revision: &str,
    label: &str,
    report: &Value,
) -> Result<PathBuf> {
    for part in [revision, label] {
        if part.is_empty() || !part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
            return Err(
                "revision/label must contain only ASCII letters, digits and hyphens".into(),
            );
        }
    }
    let directory = directory.join(revision);
    fs::create_dir_all(&directory)?;
    for index in 1..=100_000 {
        let run = directory.join(format!("{label}-{index:03}"));
        match fs::create_dir(&run) {
            Ok(()) => {
                let path = run.join("report.json");
                let mut file = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)?;
                file.write_all(&serde_json::to_vec(report)?)?;
                file.write_all(b"\n")?;
                file.sync_all()?;
                return Ok(path);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(error) => return Err(error.into()),
        }
    }
    Err("too many quality reports for this revision and label".into())
}
pub(super) fn compare(reference: &Path, metrics: &Value) -> Result<()> {
    let report: Value = serde_json::from_slice(&fs::read(reference)?)?;
    if report.get("metrics") != Some(metrics) {
        return Err(format!("quality values differ from {}", reference.display()).into());
    }
    Ok(())
}

/// Source identity independent of index state; generated history is not input.
pub(super) fn source_digest(root: &Path) -> Result<String> {
    let output = std::process::Command::new("git")
        .current_dir(root)
        .args([
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
            "--",
            "crates",
            "make",
            "gates",
            "Makefile",
            "Cargo.toml",
            "Cargo.lock",
        ])
        .output()?;
    if !output.status.success() {
        return Err("cannot enumerate quality source inputs".into());
    }
    let listing = String::from_utf8(output.stdout)?;
    let mut paths: Vec<_> = listing.split('\0').filter(|p| !p.is_empty()).collect();
    paths.sort_unstable();
    paths.dedup();
    let mut hash = blake3::Hasher::new();
    for path in paths {
        let bytes = fs::read(root.join(path))?;
        hash.update(&(path.len() as u64).to_le_bytes());
        hash.update(path.as_bytes());
        hash.update(&(bytes.len() as u64).to_le_bytes());
        hash.update(&bytes);
    }
    Ok(hash.finalize().to_hex().to_string())
}
