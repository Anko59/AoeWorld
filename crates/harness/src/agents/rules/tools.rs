//! Command rules other than Git, GitHub, Make and Docker: the environment, the
//! host toolchain, `find`, and the agents' read-mostly allow-list.
use super::variables;
use super::writers;
use crate::agents::{
    bash::{self, State},
    context::{Access, Context, Verdict},
    shell::{Join, Word},
};

/// Reconfigure Git, gh or Make for every role.
const ALWAYS: &[&str] = &[
    "HOME",
    "XDG_CONFIG_HOME",
    "GH_CONFIG_DIR",
    "MAKEFLAGS",
    "MAKEFILES",
    "MFLAGS",
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_COMMON_DIR",
    "GIT_EXEC_PATH",
    "GIT_TEMPLATE_DIR",
    "GIT_CEILING_DIRECTORIES",
    "GIT_NAMESPACE",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
];
/// Run code or move tools; agents may only set them to a no-op.
const AGENT: &[&str] = &[
    "PAGER",
    "GIT_PAGER",
    "GH_PAGER",
    "MANPAGER",
    "EDITOR",
    "VISUAL",
    "GIT_EDITOR",
    "GIT_SEQUENCE_EDITOR",
    "GH_EDITOR",
    "BROWSER",
    "GH_BROWSER",
    "GIT_SSH",
    "GIT_SSH_COMMAND",
    "GIT_ASKPASS",
    "SSH_ASKPASS",
    "GIT_EXTERNAL_DIFF",
    "GIT_INDEX_FILE",
    "LD_PRELOAD",
    "LD_LIBRARY_PATH",
    "LD_AUDIT",
    "PATH",
    "BASH_ENV",
    "ENV",
    "PROMPT_COMMAND",
    "SHELLOPTS",
    "BASHOPTS",
    "IFS",
    "CDPATH",
    "DOCKER_HOST",
    "DOCKER_CONTEXT",
    "DOCKER_CONFIG",
];

pub(crate) fn assignment(context: &Context, name: &str, value: &Word) -> Verdict {
    if ALWAYS.contains(&name)
        || (context.role.is_agent() && variables::dangerous_make_variable(name, true))
        || name.starts_with("GIT_CONFIG")
    {
        return Err(format!(
            "setting `{name}` reconfigures Git, gh or Make around the gates; run the command without it"
        ));
    }
    let noop = value.plain() && matches!(value.text.as_str(), "" | "cat" | "true" | ":");
    if context.role.is_agent() && (AGENT.contains(&name) || name.starts_with("DYLD_")) && !noop {
        return Err(format!(
            "agents do not set `{name}` (it runs code or moves tools); leave it unset or set it to `cat`"
        ));
    }
    Ok(())
}

/// Commands an agent may run, each with its own rule where it can write.
const AGENT_ALLOWED: &[&str] = &[
    "ls",
    "cat",
    "head",
    "tail",
    "wc",
    "grep",
    "egrep",
    "fgrep",
    "rg",
    "find",
    "sort",
    "uniq",
    "cut",
    "tr",
    "diff",
    "cmp",
    "comm",
    "stat",
    "file",
    "du",
    "df",
    "pwd",
    "echo",
    "printf",
    "true",
    "false",
    "test",
    "[",
    "[[",
    "]]",
    "basename",
    "dirname",
    "realpath",
    "readlink",
    "which",
    "type",
    "whereis",
    "date",
    "sleep",
    "jq",
    "yq",
    "sed",
    "awk",
    "gawk",
    "mawk",
    "nawk",
    "xxd",
    "od",
    "hexdump",
    "strings",
    "sha1sum",
    "sha256sum",
    "sha512sum",
    "md5sum",
    "b3sum",
    "cksum",
    "tree",
    "column",
    "nl",
    "paste",
    "join",
    "seq",
    "rev",
    "tac",
    "fold",
    "expand",
    "unexpand",
    "numfmt",
    "id",
    "whoami",
    "uname",
    "hostname",
    "nproc",
    "free",
    "uptime",
    "ps",
    "pgrep",
    "printenv",
    "locale",
    "getconf",
    "dirs",
    "wait",
    "exit",
    "return",
    "for",
    "base64",
    "shuf",
    "tee",
    "cp",
    "mv",
    "rm",
    "rmdir",
    "mkdir",
    "touch",
    "truncate",
    "chmod",
    "mktemp",
    "tar",
    "unzip",
    "dd",
    "curl",
    "wget",
];
/// Commands that only read, accepted behind `xargs` and `find -exec`.
const READS: &[&str] = &[
    "ls",
    "cat",
    "head",
    "tail",
    "wc",
    "grep",
    "egrep",
    "fgrep",
    "rg",
    "cut",
    "tr",
    "diff",
    "cmp",
    "comm",
    "stat",
    "file",
    "du",
    "echo",
    "printf",
    "true",
    "false",
    "test",
    "basename",
    "dirname",
    "realpath",
    "readlink",
    "jq",
    "xxd",
    "od",
    "hexdump",
    "strings",
    "sha1sum",
    "sha256sum",
    "sha512sum",
    "md5sum",
    "b3sum",
    "cksum",
    "column",
    "nl",
    "paste",
    "join",
    "rev",
    "tac",
    "fold",
    "expand",
    "numfmt",
    "uniq",
    "base64",
    "tree",
    "sort",
    "awk",
    "sed",
];

pub(crate) fn reads_only(words: &[Word]) -> bool {
    let Some(head) = words.first().filter(|w| w.plain()) else {
        return false;
    };
    let in_place = words.iter().any(|w| {
        w.text.starts_with("-i")
            || w.text.starts_with("--in-place")
            || w.text.starts_with("-o")
            || w.text.starts_with("--output")
    });
    READS.contains(&head.text.as_str()) && !in_place
}

const INTERPRETERS: &[&str] = &[
    "python",
    "pip",
    "perl",
    "ruby",
    "irb",
    "node",
    "deno",
    "bun",
    "php",
    "lua",
    "luajit",
    "rscript",
    "julia",
    "java",
    "go",
    "osascript",
    "tclsh",
    "expect",
    "pwsh",
    "powershell",
    "guile",
    "swift",
];
const NETWORK: &[&str] = &[
    "ssh", "scp", "sftp", "nc", "ncat", "netcat", "telnet", "ftp", "socat", "mosh", "rsync",
];

/// The host toolchain: AGENTS.md requires the Dockerized Make targets.
fn toolchain_hint(base: &str) -> Option<&'static str> {
    let rust = "use the Dockerized Make targets: `make fmt`, `make lint`, `make test-unit`, `make test-harness`, `make build` (see `make help`)";
    let browser =
        "use the Dockerized Make targets: `make browser-check`, `make test-e2e`, `make test-wasm`";
    match base {
        "cargo" | "rustc" | "rustup" | "rustfmt" | "rustdoc" | "clippy-driver" | "cross" => {
            Some(rust)
        }
        _ if base.starts_with("cargo-") => Some(rust),
        "npm" | "npx" | "pnpm" | "yarn" | "corepack" | "tsc" | "eslint" | "prettier"
        | "playwright" => Some(browser),
        "wasm-bindgen" | "wasm-pack" | "wasm-opt" | "trunk" => {
            Some("use `make build-wasm` or `make test-wasm`")
        }
        _ => None,
    }
}

pub(crate) fn other(context: &Context, state: &mut State, base: &str, rest: &[Word]) -> Verdict {
    let agent = context.role.is_agent();
    if let Some(hint) = toolchain_hint(base) {
        return Err(format!("`{base}` is not run on the host: {hint}"));
    }
    let lower = base.to_ascii_lowercase();
    if agent && INTERPRETERS.iter().any(|i| lower.starts_with(i)) {
        return Err(format!(
            "agents do not run interpreters (`{base}`): the policy cannot see the code they run. Use Read/Grep/Edit or a Make target"
        ));
    }
    if agent && NETWORK.contains(&base) {
        return Err(format!("agents do not use network tools (`{base}`)"));
    }
    match base {
        "find" => return find(context, state, rest),
        "ln" if agent => return Err("agents do not create links".into()),
        "patch" if agent => {
            return Err(
                "agents do not apply patches (the patch names its own files); use the Edit tool"
                    .into(),
            );
        }
        _ => {}
    }
    writers::judge(context, state.cwd.as_deref(), base, rest)?;
    if agent && !AGENT_ALLOWED.contains(&base) {
        return Err(format!(
            "`{base}` is not on the agent command allow-list; use the dedicated tools, a Make target, or ask the main session"
        ));
    }
    Ok(())
}

fn find(context: &Context, state: &mut State, rest: &[Word]) -> Verdict {
    let split = rest
        .iter()
        .position(|w| w.text.starts_with('-') || matches!(w.text.as_str(), "(" | "!" | ")"))
        .unwrap_or(rest.len());
    let dot = [Word::literal(".")];
    let roots = if split == 0 { &dot[..] } else { &rest[..split] };
    let mut index = split;
    while let Some(word) = rest.get(index) {
        index += 1;
        match word.text.as_str() {
            "-exec" | "-execdir" | "-ok" | "-okdir" => {
                let start = index;
                while rest
                    .get(index)
                    .is_some_and(|w| w.text != ";" && w.text != "+")
                {
                    index += 1;
                }
                let inner = &rest[start..index.min(rest.len())];
                index += 1;
                if inner.is_empty() {
                    return Err("`find -exec` without a command".into());
                }
                if context.role.is_agent() && !reads_only(inner) {
                    return Err(format!(
                        "`find -exec {}` hides which paths it changes; list the paths explicitly",
                        inner[0].text
                    ));
                }
                bash::dispatch(context, state, inner, Join::Sequence)?;
            }
            "-delete" => {
                for root in roots {
                    context.write(state.cwd.as_deref(), root, Access::Remove)?;
                }
            }
            "-fprint" | "-fprint0" | "-fls" | "-fprintf" => {
                if let Some(target) = rest.get(index) {
                    context.write(state.cwd.as_deref(), target, Access::Put)?;
                }
                index += if word.text == "-fprintf" { 2 } else { 1 };
            }
            _ => {}
        }
    }
    Ok(())
}
