//! Each runtime names its tools differently; this turns one tool call into
//! the few shapes the policy judges. Claude Code and pi (whose extension sends
//! Claude-shaped JSON) use `Bash`/`Edit`/`Write`; Codex uses `Bash` and
//! `apply_patch` (the patch text names its files); DeepSeek Harness uses
//! `bash` (with `workdir`), `write`, `edit` and `str_replace_editor`.
use super::context::Access;
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum Runtime {
    Claude,
    Codex,
    Dsh,
    Pi,
}

impl Runtime {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Codex => "Codex",
            Self::Dsh => "DeepSeek Harness",
            Self::Pi => "pi",
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Call {
    /// A shell line, run from `cwd` when the runtime names one.
    Shell {
        command: String,
        cwd: Option<PathBuf>,
        /// The runtime may run the line elsewhere than `cwd` says: Codex's
        /// `workdir` never reaches its hooks. Agents then name writes absolutely.
        cwd_uncertain: bool,
        /// The shell may persist `cd` across calls (dsh's persistent preset), so
        /// agents pass `workdir` instead of changing directory.
        forbid_cd: bool,
    },
    /// Files the call creates, changes or deletes.
    Writes(Vec<(String, Access)>),
    /// A call whose effects cannot be read from its input.
    Opaque(&'static str),
    /// Reads, searches and everything else the policy does not judge.
    Other,
}

fn text<'a>(input: &'a Value, key: &str) -> Option<&'a str> {
    input.get(key).and_then(Value::as_str)
}

fn malformed(what: &'static str) -> Call {
    Call::Opaque(what)
}

pub(crate) fn call(runtime: Runtime, tool: &str, input: &Value, cwd: Option<&Path>) -> Call {
    let put = |key: &str, what| match text(input, key) {
        Some(path) => Call::Writes(vec![(path.to_owned(), Access::Put)]),
        None => malformed(what),
    };
    match (runtime, tool) {
        (_, "Bash") | (Runtime::Dsh | Runtime::Pi, "bash") => match text(input, "command") {
            Some(command) => Call::Shell {
                command: command.to_owned(),
                cwd: workdir(input, cwd),
                cwd_uncertain: runtime == Runtime::Codex,
                forbid_cd: runtime == Runtime::Dsh,
            },
            None => malformed("a shell call without a command string"),
        },
        (_, "Edit" | "Write" | "MultiEdit") | (Runtime::Dsh, "write" | "edit") => {
            put("file_path", "an edit without a file path")
        }
        (Runtime::Pi, "write" | "edit") => match text(input, "path").map(pi_path) {
            Some(Ok(path)) => Call::Writes(vec![(path, Access::Put)]),
            Some(Err(why)) => Call::Opaque(why),
            None => malformed("an edit without a path"),
        },
        (_, "NotebookEdit") => put("notebook_path", "a notebook edit without a path"),
        (Runtime::Dsh, "str_replace_editor") => match text(input, "command") {
            Some("view") => Call::Other,
            _ => put("path", "an editor call without a path"),
        },
        (Runtime::Codex, "apply_patch") => match text(input, "command") {
            Some(patch) => Call::Writes(patch_targets(patch)),
            None => malformed("a patch without text"),
        },
        (Runtime::Dsh, "pwsh" | "run_code" | "workflow") | (Runtime::Pi, "powershell") => {
            Call::Opaque(
                "PowerShell, run_code and workflow calls run code the policy cannot read; use bash",
            )
        }
        _ => Call::Other,
    }
}

/// DeepSeek Harness runs each command in a fresh shell from `workdir`.
fn workdir(input: &Value, cwd: Option<&Path>) -> Option<PathBuf> {
    match text(input, "workdir") {
        Some(dir) if Path::new(dir).is_absolute() => Some(PathBuf::from(dir)),
        Some(dir) => cwd.map(|cwd| cwd.join(dir)),
        None => cwd.map(Path::to_path_buf),
    }
}

/// Paths an `apply_patch` envelope adds, updates, moves or deletes.
pub(crate) fn patch_targets(patch: &str) -> Vec<(String, Access)> {
    let mut targets = Vec::new();
    for line in patch.lines() {
        // Codex trims a hunk header on both sides before matching it.
        let line = line.trim();
        let target = [
            ("*** Add File: ", Access::Put),
            ("*** Update File: ", Access::Put),
            ("*** Move to: ", Access::Put),
            ("*** Delete File: ", Access::Remove),
        ]
        .into_iter()
        .find_map(|(prefix, access)| line.strip_prefix(prefix).map(|path| (path, access)));
        if let Some((path, access)) = target {
            targets.push((path.trim().to_owned(), access));
        }
    }
    targets
}

/// pi resolves a leading `@` away and turns `file://` URLs into paths before it
/// writes (`resolveToCwd`), so the judge must see the same path.
fn pi_path(path: &str) -> Result<String, &'static str> {
    let path = path.strip_prefix('@').unwrap_or(path);
    if path.starts_with("file:") {
        return Err("pi file: URLs are not judged; give a plain path");
    }
    Ok(path.to_owned())
}
