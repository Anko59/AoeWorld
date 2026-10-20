//! Exact changed bytes, ignoring context, offsets and hunk regrouping. Unlike
//! whitespace-normalized patch IDs, this preserves changes inside literals.
use super::Result;
use serde::Serialize;
use std::{ffi::OsStr, os::unix::ffi::OsStrExt, path::Path, process::Command};

#[derive(Serialize)]
struct Line {
    bytes: Vec<u8>,
    newline: bool,
}

#[derive(Serialize)]
struct File {
    path: Vec<u8>,
    old_mode: Vec<u8>,
    new_mode: Vec<u8>,
    removed: Vec<Line>,
    added: Vec<Line>,
    binary: Option<(Vec<u8>, Vec<u8>)>,
}

fn output(command: &mut Command) -> Result<Vec<u8>> {
    let out = command
        .env("LC_ALL", "C")
        .output()
        .map_err(|e| format!("git diff: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git diff: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(out.stdout)
}

pub(super) fn identity(root: &Path, from: &str, to: &str) -> Result<Vec<u8>> {
    let range = format!("{from}..{to}");
    // Raw metadata gives modes and byte-exact paths, without Git's quoting.
    let raw = output(Command::new("git").arg("-C").arg(root).args([
        "diff",
        "--raw",
        "--no-abbrev",
        "-z",
        "--no-renames",
        "--no-ext-diff",
        "--no-textconv",
        "--ignore-submodules=none",
        &range,
        "--",
    ]))?;
    let entries: Vec<_> = raw.split(|b| *b == 0).collect();
    if entries.last() != Some(&b"".as_slice()) || entries.len() % 2 != 1 {
        return Err("malformed raw diff metadata".into());
    }
    let mut files = Vec::new();
    for pair in entries[..entries.len() - 1].as_chunks::<2>().0 {
        let header = pair[0].strip_prefix(b":").ok_or("malformed diff header")?;
        let fields: Vec<_> = header.split(|b| *b == b' ').collect();
        if fields.len() != 5 || pair[1].is_empty() {
            return Err("malformed diff entry".into());
        }
        let diff = output(
            Command::new("git")
                .arg("-C")
                .arg(root)
                .args([
                    "--literal-pathspecs",
                    "diff",
                    "--no-renames",
                    "--no-ext-diff",
                    "--no-textconv",
                    "--no-color",
                    "--diff-algorithm=myers",
                    "--no-indent-heuristic",
                    "--unified=0",
                    "--binary",
                    "--full-index",
                    &range,
                    "--",
                ])
                .arg(OsStr::from_bytes(pair[1])),
        )?;
        let mut file = File {
            path: pair[1].to_vec(),
            old_mode: fields[0].to_vec(),
            new_mode: fields[1].to_vec(),
            removed: Vec::new(),
            added: Vec::new(),
            binary: None,
        };
        if fields[0] == b"160000"
            || fields[1] == b"160000"
            || diff
                .split(|b| *b == b'\n')
                .any(|line| line == b"GIT binary patch")
        {
            // Binary edits and gitlinks are atomic: both object IDs must match.
            file.binary = Some((fields[2].to_vec(), fields[3].to_vec()));
        } else {
            hunks(&diff, &mut file)?;
        }
        files.push(file);
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    serde_json::to_vec(&files).map_err(|e| e.to_string())
}

fn hunks(diff: &[u8], file: &mut File) -> Result<()> {
    let mut in_hunk = false;
    let mut last = None;
    for line in diff.split(|b| *b == b'\n') {
        if line.starts_with(b"@@ ") {
            in_hunk = true;
            last = None;
            continue;
        }
        if !in_hunk {
            continue;
        }
        match line.first() {
            Some(b'-') => {
                file.removed.push(Line {
                    bytes: line[1..].to_vec(),
                    newline: true,
                });
                last = Some(false);
            }
            Some(b'+') => {
                file.added.push(Line {
                    bytes: line[1..].to_vec(),
                    newline: true,
                });
                last = Some(true);
            }
            Some(b'\\') if line == b"\\ No newline at end of file" => {
                let lines = match last {
                    Some(true) => &mut file.added,
                    Some(false) => &mut file.removed,
                    None => return Err("newline marker without a changed line".into()),
                };
                lines
                    .last_mut()
                    .ok_or("newline marker without a changed line")?
                    .newline = false;
            }
            _ => {}
        }
    }
    Ok(())
}
