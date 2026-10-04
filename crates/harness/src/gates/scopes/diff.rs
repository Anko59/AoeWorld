use super::*;
use crate::process::{self, Cancellation, CaptureExit};
use std::time::{Duration, Instant};
fn bounded(
    root: &Path,
    args: &[&str],
    start: Instant,
    cancellation: &Cancellation,
) -> Result<Vec<u8>> {
    let budget = Duration::from_secs(30)
        .saturating_sub(start.elapsed())
        .min(Duration::from_secs(5));
    let mut argv = vec![
        "--no-replace-objects",
        "--literal-pathspecs",
        "-c",
        "core.fsmonitor=false",
    ];
    argv.extend_from_slice(args);
    let captured = process::capture_in(
        root,
        "git",
        &argv,
        &[
            ("GIT_CONFIG_NOSYSTEM", "1"),
            ("GIT_CONFIG_GLOBAL", "/dev/null"),
            ("GIT_ATTR_NOSYSTEM", "1"),
            ("GIT_OPTIONAL_LOCKS", "0"),
        ],
        budget,
        cancellation,
    );
    if !matches!(captured.exit, CaptureExit::Success) || captured.truncated {
        return Err("immutable diff unavailable: failed, cancelled, timed out or exceeds 64KiB per receipt; split task".into());
    }
    Ok(captured.stdout)
}
#[derive(Serialize)]
pub(crate) struct Diff {
    pub(crate) paths: Vec<String>,
    pub(crate) files: Vec<FileDiff>,
}
#[derive(Serialize)]
pub(crate) struct FileDiff {
    pub(crate) path: String,
    pub(crate) removed: Vec<String>,
    pub(crate) added: Vec<String>,
    pub(crate) binary: bool,
}
impl Snapshot {
    /// Endpoint-checked immutable comparison. Source Git remains read-only;
    /// ext-diff/textconv are disabled. Local Git probes are not wall-supervised.
    pub(crate) fn diff_from(&self, base: &Snapshot, cancellation: &Cancellation) -> Result<Diff> {
        let start = Instant::now();
        let (Kind::Commit(candidate), Kind::Commit(base_oid)) =
            (&self.identity.kind, &base.identity.kind)
        else {
            return Err("task comparison requires immutable commits".into());
        };
        if self.source != base.source {
            return Err("comparison requires one original object store".into());
        }
        self.verify_source()?;
        base.verify_source()?;
        let names = bounded(
            &self.source,
            &[
                "diff",
                "--no-ext-diff",
                "--no-textconv",
                "--no-renames",
                "--name-only",
                "-z",
                base_oid,
                candidate,
                "--",
            ],
            start,
            cancellation,
        )?;
        let mut paths = BTreeSet::new();
        for name in names
            .split(|byte| *byte == 0)
            .filter(|name| !name.is_empty())
        {
            let name = std::str::from_utf8(name)?;
            relative(name)?;
            paths.insert(name.to_owned());
        }
        if paths.len() > 4096 {
            return Err("comparison exceeds 4096 paths".into());
        }
        let mut files = Vec::new();
        let mut bytes = 0usize;
        for path in &paths {
            let raw = bounded(
                &self.source,
                &[
                    "diff",
                    "--no-ext-diff",
                    "--no-textconv",
                    "--no-renames",
                    "--no-color",
                    "--unified=0",
                    "--text",
                    "--diff-algorithm=myers",
                    "--no-indent-heuristic",
                    base_oid,
                    candidate,
                    "--",
                    path,
                ],
                start,
                cancellation,
            )?;
            bytes = bytes.checked_add(raw.len()).ok_or("diff size overflow")?;
            if bytes > 2 * 1024 * 1024 {
                return Err("complete diff exceeds 2MiB planning limit; split task".into());
            }
            let text = std::str::from_utf8(&raw)?;
            let mut file = FileDiff {
                path: path.clone(),
                removed: vec![],
                added: vec![],
                binary: false,
            };
            let mut in_hunk = false;
            for line in text.lines() {
                if line.starts_with("Binary files ") {
                    file.binary = true;
                }
                if line.starts_with("@@ ") {
                    in_hunk = true;
                    continue;
                }
                if !in_hunk {
                    continue;
                }
                if let Some(line) = line.strip_prefix('-') {
                    file.removed.push(line.to_owned());
                }
                if let Some(line) = line.strip_prefix('+') {
                    file.added.push(line.to_owned());
                }
            }
            files.push(file);
        }
        self.verify_source()?;
        base.verify_source()?;
        Ok(Diff {
            paths: paths.into_iter().collect(),
            files,
        })
    }
}
