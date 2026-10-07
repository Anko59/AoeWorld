//! Make and Docker, the dispatchers every check runs through.
use crate::claude::{
    args::{self, Spec},
    context::{Context, Verdict},
    role::Role,
    shell::Word,
};

/// Make targets that belong to a person or change shared setup.
const PERSON_TARGETS: &[&str] = &[
    "hooks-install",
    "bootstrap",
    "claude-hook-build",
    "release-publish",
    "release-build",
    "repo-policy-check",
];
/// Variables that redirect what a Make target runs.
fn dangerous_make_variable(name: &str) -> bool {
    matches!(
        name,
        "SHELL" | "MAKEFLAGS" | "MAKEFILES" | "ROOT" | "UID" | "GID" | ".SHELLFLAGS"
    ) || name.ends_with("_IMAGE")
        || name.ends_with("_RUN")
        || name.ends_with("_MOUNTS")
        || name.ends_with("_MOUNT")
        || name.starts_with("GIT_")
        || name.ends_with("_REAL")
}

pub(crate) fn make(context: &Context, rest: &[Word]) -> Verdict {
    let spec = Spec {
        short: "CfIjlWoO",
        long: &[
            "directory",
            "file",
            "makefile",
            "include-dir",
            "jobs",
            "load-average",
            "what-if",
            "new-file",
            "assume-new",
            "old-file",
            "assume-old",
            "eval",
            "output-sync",
        ],
        stop_at_positional: false,
    };
    let parsed = args::split(rest, &spec);
    if parsed.has(&[
        "-f",
        "--file",
        "--makefile",
        "--eval",
        "-e",
        "--environment-overrides",
        "-i",
        "--ignore-errors",
    ]) {
        return Err(
            "run the repository Makefile as written: no `-f`, `--eval`, `-e` or `-i`".into(),
        );
    }
    if parsed.has(&[
        "-o",
        "-W",
        "--old-file",
        "--assume-old",
        "--what-if",
        "--new-file",
        "--assume-new",
        "-t",
        "--touch",
        "-q",
        "--question",
    ]) {
        return Err("do not pretend Make targets are up to date; run them".into());
    }
    let agent = context.role.is_agent();
    let mut targets = Vec::new();
    for word in &parsed.positionals {
        if !word.plain() && agent {
            return Err(format!(
                "the Make argument `{}` is computed at run time",
                word.text
            ));
        }
        match word.text.split_once('=') {
            Some((name, _)) if dangerous_make_variable(name.trim_end_matches([':', '+', '?'])) => {
                return Err(format!(
                    "overriding `{name}` changes what the gates run; use the Makefile's own value"
                ));
            }
            Some((name, _)) if agent && !agent_make_variable(name) => {
                return Err(format!(
                    "agents set only `AOE_*`, `HARNESS_*` and `REVIEW_*` Make variables, not `{name}`"
                ));
            }
            Some(_) => {}
            None => targets.push(word.text.as_str()),
        }
    }
    for target in targets {
        if agent && PERSON_TARGETS.contains(&target) {
            return Err(format!(
                "`make {target}` belongs to the main session or a person"
            ));
        }
        if context.role == Role::Reviewer && !reviewer_target(target) {
            return Err(format!(
                "reviewers run checks only; `make {target}` is not a check"
            ));
        }
    }
    Ok(())
}

fn agent_make_variable(name: &str) -> bool {
    name.starts_with("AOE_") || name.starts_with("HARNESS_") || name.starts_with("REVIEW_")
}

fn reviewer_target(target: &str) -> bool {
    target.ends_with("-check")
        || matches!(
            target,
            "help"
                | "doctor"
                | "lint"
                | "deny"
                | "test-unit"
                | "test-harness"
                | "test-wasm"
                | "pre-commit"
                | "preflight"
                | "gate-plan"
                | "scope-check"
                | "ci-select"
                | "build"
                | "build-wasm"
                | "map-test"
                | "geodata-test"
                | "status"
                | "logs"
        )
}

pub(crate) fn docker(context: &Context, base: &str, rest: &[Word]) -> Verdict {
    if !context.role.is_agent() {
        return Ok(());
    }
    let words: Vec<&str> = rest.iter().map(|w| w.text.as_str()).collect();
    if words.first().is_some_and(|w| w.starts_with('-')) {
        return Err(format!("agents run `{base}` without global options"));
    }
    let read = match words.as_slice() {
        [
            "ps" | "images" | "version" | "info" | "logs" | "inspect" | "stats",
            ..,
        ] => words[0] != "stats" || words.contains(&"--no-stream"),
        [
            "image" | "container" | "volume" | "network" | "context",
            "ls" | "list" | "inspect" | "show",
            ..,
        ] => true,
        ["system", "df", ..] => true,
        ["compose", "ps" | "logs" | "ls", ..] => true,
        _ => base == "docker-compose" && matches!(words.first(), Some(&"ps" | &"logs")),
    };
    if read {
        Ok(())
    } else {
        Err("agents use Docker only through Make targets; read-only `docker ps/images/logs/inspect` are allowed".into())
    }
}
