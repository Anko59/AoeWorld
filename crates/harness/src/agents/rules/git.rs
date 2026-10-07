//! Git rules. No Claude session bypasses a hook or moves `dev`/`main`, and
//! duty-bound agents only read Git state.
use super::SHIP;
use crate::agents::{
    args::{self, Spec},
    bash::{self, State},
    context::{Access, Context, Verdict},
    role::Role,
    shell::Word,
};

const PROTECTED: &[&str] = &["dev", "main"];

pub(crate) fn protected_branch(name: &str) -> bool {
    let name = name.strip_prefix("refs/heads/").unwrap_or(name);
    PROTECTED.contains(&name) || name.starts_with("release/")
}

/// Read-only subcommands, the only ones duty-bound agents may run.
const GIT_READS: &[&str] = &[
    "status",
    "log",
    "show",
    "diff",
    "blame",
    "annotate",
    "rev-parse",
    "rev-list",
    "ls-files",
    "ls-tree",
    "cat-file",
    "merge-base",
    "describe",
    "shortlog",
    "grep",
    "show-ref",
    "for-each-ref",
    "name-rev",
    "patch-id",
    "diff-tree",
    "diff-files",
    "diff-index",
    "whatchanged",
    "range-diff",
    "check-ignore",
    "check-attr",
    "count-objects",
    "help",
    "version",
    "var",
    "branch",
    "tag",
    "remote",
    "config",
    "worktree",
    "stash",
];

pub(crate) fn git(context: &Context, state: &mut State, rest: &[Word]) -> Verdict {
    let agent = context.role.is_agent();
    let mut index = 0;
    let mut cwd = state.cwd.clone();
    while let Some(word) = rest.get(index) {
        let text = word.text.as_str();
        if !text.starts_with('-') {
            break;
        }
        match text {
            "-C" => {
                let target = rest.get(index + 1).ok_or("`git -C` without a directory")?;
                cwd = context.resolve(cwd.as_deref(), target)?;
                index += 2;
            }
            "--no-pager"
            | "-P"
            | "--paginate"
            | "-p"
            | "--no-optional-locks"
            | "--literal-pathspecs"
            | "--no-replace-objects"
            | "--glob-pathspecs"
            | "--noglob-pathspecs"
            | "--icase-pathspecs"
            | "--bare"
            | "--version"
            | "--help" => index += 1,
            "-c" | "--config-env" => {
                return Err(
                    "`git -c` overrides configuration around the hooks; run git without it".into(),
                );
            }
            _ if text.starts_with("--config-env=") => {
                return Err(
                    "`git --config-env` overrides configuration; run git without it".into(),
                );
            }
            _ if text.starts_with("--git-dir")
                || text.starts_with("--work-tree")
                || text.starts_with("--namespace")
                || text.starts_with("--exec-path")
                || text.starts_with("--super-prefix") =>
            {
                return Err(format!(
                    "`git {text}` points git elsewhere; use `git -C <checkout>`"
                ));
            }
            _ => {
                return Err(format!(
                    "unknown git global option `{text}`; write the subcommand plainly"
                ));
            }
        }
    }
    let Some(sub) = rest.get(index) else {
        return Ok(());
    };
    if !sub.plain() {
        return if agent {
            Err("the git subcommand is computed at run time".into())
        } else {
            Ok(())
        };
    }
    let args = &rest[index + 1..];
    let sub = sub.text.as_str();
    let parsed = args::split(
        args,
        &Spec {
            short: "mFCctSXseo",
            long: &[
                "message",
                "file",
                "reuse-message",
                "reedit-message",
                "template",
                "author",
                "date",
                "exec",
                "strategy",
                "strategy-option",
                "push-option",
                "receive-pack",
                "repo",
                "upload-pack",
                "onto",
                "cleanup",
                "fixup",
                "squash",
                "trailer",
            ],
            stop_at_positional: false,
        },
    );
    let no_verify = parsed.has_long("--no-verify", 6) || (sub == "commit" && parsed.has(&["-n"]));
    if no_verify {
        return Err("hooks are never bypassed (`--no-verify`/`-n`); fix what they report".into());
    }
    if context.role.duty_bound() {
        return duty_bound_git(context.role, sub, args, &parsed);
    }
    match sub {
        "push" | "send-pack" => Err(format!("`git {sub}` is not run directly; {SHIP}")),
        "subtree" if parsed.positionals.iter().any(|w| w.text == "push") => Err(format!(
            "`git subtree push` publishes without evidence; {SHIP}"
        )),
        "update-ref" | "symbolic-ref"
            if args
                .iter()
                .any(|w| protected_branch(&w.text) || w.text == "--stdin")
                || agent =>
        {
            Err(format!("`git {sub}` would move a branch by hand; {SHIP}"))
        }
        "branch" | "checkout" | "switch" => {
            let moves = parsed.has(&[
                "-f", "--force", "-D", "-d", "--delete", "-M", "-m", "--move", "-C", "-c",
                "--copy", "-B",
            ]);
            let values = parsed.options.iter().filter_map(|o| o.value.as_ref());
            let named = parsed.positionals.iter().copied().chain(values);
            if moves && named.into_iter().any(|w| protected_branch(&w.text)) {
                return Err(format!(
                    "`dev`, `main` and `release/*` move only by merging a pull request; {SHIP}"
                ));
            }
            if sub == "checkout" {
                pathspecs(context, cwd.as_deref(), args)?;
            }
            Ok(())
        }
        "reset" => pathspecs(context, cwd.as_deref(), args),
        "config" => config(agent, &parsed),
        "apply" | "am" if agent => Err(
            "agents do not apply patches (the patch names its own files); use the Edit tool".into(),
        ),
        "clean" | "stash" | "filter-branch" | "filter-repo" | "replace" | "gc" | "prune"
        | "worktree"
            if agent
                && !(sub == "worktree"
                    && parsed.positionals.first().is_some_and(|w| w.text == "list")) =>
        {
            Err(format!(
                "agents do not run `git {sub}`; ask the main session"
            ))
        }
        "restore" | "rm" | "mv" => pathspecs(context, cwd.as_deref(), args),
        "rebase" => {
            for command in parsed.values(&["-x", "--exec"]) {
                bash::line(context, &command.text, cwd.clone(), 1)?;
            }
            Ok(())
        }
        "tag" if agent && !(parsed.positionals.is_empty() || parsed.has(&["-l", "--list"])) => {
            Err("agents only list tags".into())
        }
        _ => Ok(()),
    }
}

fn duty_bound_git(role: Role, sub: &str, args: &[Word], parsed: &args::Args<'_>) -> Verdict {
    let refuse = || {
        Err(format!(
            "the {} does not change Git state (`git {sub}`); report back and the main session commits and ships",
            role.name()
        ))
    };
    if !GIT_READS.contains(&sub) {
        return refuse();
    }
    let positional = args::texts(&parsed.positionals);
    let listing = match sub {
        "branch" => {
            positional.is_empty()
                && !parsed.has(&[
                    "-d",
                    "-D",
                    "-m",
                    "-M",
                    "-c",
                    "-C",
                    "-f",
                    "--delete",
                    "--move",
                    "--copy",
                    "--force",
                    "-u",
                    "--set-upstream-to",
                    "--unset-upstream",
                    "--edit-description",
                ])
        }
        "tag" => positional.is_empty() || parsed.has(&["-l", "--list"]),
        "remote" => positional
            .first()
            .is_none_or(|v| matches!(*v, "show" | "get-url")),
        "config" => reading_config(parsed),
        "worktree" => positional.first() == Some(&"list"),
        "stash" => matches!(positional.first(), Some(&"list" | &"show")),
        _ => true,
    };
    let output = args.iter().any(|w| {
        w.text.starts_with("--output")
            || w.text.starts_with("--ext-diff")
            || w.text.starts_with("--textconv")
    });
    if listing && !output { Ok(()) } else { refuse() }
}

fn reading_config(parsed: &args::Args<'_>) -> bool {
    let reads = [
        "--get",
        "--get-all",
        "--get-regexp",
        "--list",
        "-l",
        "--get-urlmatch",
        "--show-origin",
        "--show-scope",
        "--name-only",
        "--null",
        "-z",
        "--local",
        "--global",
        "--system",
        "--worktree",
        "--includes",
        "--no-includes",
        "--type",
        "--bool",
        "--int",
        "--path",
        "--default",
    ];
    let positional = args::texts(&parsed.positionals);
    parsed
        .options
        .iter()
        .all(|o| reads.contains(&o.name.as_str()))
        && (parsed.has(&[
            "--get",
            "--get-all",
            "--get-regexp",
            "--list",
            "-l",
            "--get-urlmatch",
        ]) || matches!(positional.first(), Some(&"get" | &"list"))
            || positional.len() == 1)
}

/// Keys whose values git runs as commands or that load other files.
fn dangerous_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key.starts_with("alias.")
        || key.starts_with("include")
        || key.starts_with("filter.")
        || key.starts_with("core.hookspath")
        || key == "core.pager"
        || key == "core.editor"
        || key == "core.fsmonitor"
        || key == "core.sshcommand"
        || key == "core.askpass"
        || key == "core.gitproxy"
        || key == "init.templatedir"
        || key == "sequence.editor"
        || key == "diff.external"
        || key.ends_with(".textconv")
        || key.ends_with(".command")
        || (key.starts_with("trailer.") && key.ends_with(".cmd"))
        || key.starts_with("pager.")
        || key == "credential.helper"
        || key == "gpg.program"
        || key.starts_with("gpg.")
        || key.starts_with("url.")
        || key.starts_with("remote.") && key.ends_with("pushurl")
        || key == "push.default"
        || key.starts_with("branch.") && key.ends_with(".merge")
        || key.starts_with("receive.")
        || key.starts_with("uploadpack.")
}

fn config(agent: bool, parsed: &args::Args<'_>) -> Verdict {
    if reading_config(parsed) {
        return Ok(());
    }
    if agent {
        return Err("agents only read git configuration".into());
    }
    if parsed.has(&["-e", "--edit", "-f", "--file", "--blob"]) {
        return Err("edit git configuration one plain key at a time".into());
    }
    let positional = args::texts(&parsed.positionals);
    let key = match positional.as_slice() {
        ["set" | "unset", key, ..] => Some(*key),
        [key, ..] => Some(*key),
        [] => None,
    };
    match key {
        Some(key) if dangerous_key(key) => Err(format!(
            "`{key}` makes git run commands, load files or change where pushes go; ask the person to set it in their own terminal"
        )),
        _ => Ok(()),
    }
}

/// Paths `git checkout -- …`, `restore`, `rm` and `mv` overwrite or delete.
fn pathspecs(context: &Context, cwd: Option<&std::path::Path>, args: &[Word]) -> Verdict {
    if !context.role.is_agent() {
        return Ok(());
    }
    let after = args
        .iter()
        .position(|w| w.text == "--")
        .map_or(args, |at| &args[at + 1..]);
    after
        .iter()
        .filter(|w| !w.text.starts_with('-'))
        .try_for_each(|w| context.write(cwd, w, Access::Remove))
}
