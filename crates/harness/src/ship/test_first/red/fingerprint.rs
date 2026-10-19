//! Red carry binds changed bytes to surrounding code and hunk function headers.
//! Review's context-free identity remains separate and unchanged.
use super::git;
use sha2::{Digest, Sha256};
use std::{path::Path, process::Command};

pub(super) fn compute(root: &Path, base: &str, head: &str) -> git::Result<(String, String)> {
    let (merge_base, change) = git::change_fingerprint_from(root, base, head)?;
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "diff",
            "--no-renames",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            "--no-relative",
            "--src-prefix=a/",
            "--dst-prefix=b/",
            "--diff-algorithm=myers",
            "--no-indent-heuristic",
            "--ignore-submodules=none",
            "--unified=50",
            "--binary",
            "--full-index",
            &merge_base,
            head,
            "--",
        ])
        .env("LC_ALL", "C")
        .output()
        .map_err(|e| format!("red carry diff: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "red carry diff: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let mut hash = Sha256::new();
    // The review identity also retains byte-exact paths, modes and binary/gitlink
    // object IDs; never depend on lossy UTF-8 decoding of diff output.
    hash.update(b"red carry context\0");
    hash.update(change.as_bytes());
    for line in output.stdout.split_inclusive(|b| *b == b'\n') {
        if line.starts_with(b"index ") {
            // Text blob IDs change after unrelated upstream insertions. Binary
            // identities remain bound by `change` and the binary patch itself.
            continue;
        }
        if let Some(hunk) = line.strip_prefix(b"@@ ") {
            let end = hunk
                .windows(3)
                .position(|bytes| bytes == b" @@")
                .ok_or("red carry: malformed hunk header")?;
            // Ignore only numeric ranges, keeping the trailing function header.
            // Unrecognized headers refuse carry instead of dropping context.
            let ranges: Vec<_> = hunk[..end].split(|b| *b == b' ').collect();
            if ranges.len() != 2 || !range(ranges[0], b'-') || !range(ranges[1], b'+') {
                return Err("red carry: malformed hunk ranges".into());
            }
            hash.update(b"@@");
            hash.update(&hunk[end + 3..]);
        } else {
            hash.update(line);
        }
    }
    let carry = hash.finalize().iter().map(|b| format!("{b:02x}")).collect();
    Ok((change, carry))
}

fn range(bytes: &[u8], sign: u8) -> bool {
    bytes.first() == Some(&sign)
        && (1..=2).contains(&bytes[1..].split(|b| *b == b',').count())
        && bytes[1..]
            .split(|b| *b == b',')
            .all(|part| !part.is_empty() && part.iter().all(u8::is_ascii_digit))
}
