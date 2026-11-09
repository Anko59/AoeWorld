//! The editable manifest can only spend allowances already present at the merge base.
use super::*;
use crate::gates::scopes::{Kind, Snapshot};
use std::process::Command;

pub(super) fn baseline(root: &Path) -> Result<Allowlist, Box<dyn Error>> {
    let mut command = Command::new("git");
    command.current_dir(root);
    for (name, _) in std::env::vars_os() {
        if name.as_encoded_bytes().starts_with(b"GIT_") {
            command.env_remove(name);
        }
    }
    let output = command
        .args([
            "--no-replace-objects",
            "merge-base",
            "HEAD",
            "refs/remotes/origin/dev",
        ])
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "cap ratchet base unavailable: fetch origin/dev: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let base = String::from_utf8(output.stdout)?.trim().to_owned();
    let snapshot = Snapshot::prepare_independent(root, Kind::Commit(base))?;
    snapshot.run_checked(|root| {
        match fs::read_to_string(root.join(ALLOWLIST)) {
            Ok(text) => Ok(serde_json::from_str(&text)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                // Initial adoption: only caps actually in the base sources can be allowed.
                Ok(serde_json::from_str(&render(&scan(root)?))?)
            }
            Err(error) => Err(error.into()),
        }
    })
}

fn counts(list: &Allowlist) -> BTreeMap<(&str, &str, &str), usize> {
    let mut counts = BTreeMap::new();
    for entry in &list.entries {
        let count = counts
            .entry((
                entry.file.as_str(),
                entry.kind.as_str(),
                entry.expr.as_str(),
            ))
            .or_insert(0usize);
        *count = count.saturating_add(entry.count);
    }
    counts
}

pub(super) fn compare(current: &Allowlist, base: &Allowlist) -> Vec<String> {
    let base = counts(base);
    counts(current).into_iter().filter_map(|(key, count)| {
        let previous = base.get(&key).copied().unwrap_or(0);
        (count > previous).then(|| format!("{ALLOWLIST}: `{} {} {}` allowance increased from base {previous} to {count}; the allowlist only shrinks", key.0, key.1, key.2))
    }).collect()
}
