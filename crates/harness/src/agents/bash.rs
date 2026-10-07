//! Judges one Bash tool call: every simple command, every write it names and
//! every command it hands to another (`sh -c`, `xargs`, `find -exec`, `$(..)`).
//! The first denial wins and says what to do instead.
use super::{
    args::{self, Spec},
    context::{Access, Context, Verdict},
    paths,
    rules::{dispatch, git, github, tools},
    shell::{self, Item, Join, Simple, Word},
};
use std::path::{Path, PathBuf};

const MAX_DEPTH: usize = 8;

pub(crate) struct State {
    pub(crate) cwd: Option<PathBuf>,
    depth: usize,
}

pub(crate) fn judge(context: &Context, command: &str, cwd: Option<&Path>) -> Verdict {
    line(context, command, cwd.map(Path::to_path_buf), 0)
}

pub(crate) fn line(context: &Context, text: &str, cwd: Option<PathBuf>, depth: usize) -> Verdict {
    let agent = context.role.is_agent();
    if depth > MAX_DEPTH {
        return if agent {
            Err("commands nested more than 8 deep cannot be judged; write it plainly".into())
        } else {
            Ok(())
        };
    }
    let parsed = match shell::parse(text) {
        Ok(parsed) => parsed,
        Err(error) if agent => {
            return Err(format!(
                "the policy cannot parse this command ({error}); write it plainly"
            ));
        }
        Err(_) => return Ok(()),
    };
    if let (Some(reason), true) = (parsed.opaque, agent) {
        return Err(format!("{reason}; write it plainly"));
    }
    for substitution in &parsed.substitutions {
        line(context, substitution, cwd.clone(), depth + 1)?;
    }
    let mut state = State { cwd, depth };
    let mut saved = Vec::new();
    for item in &parsed.items {
        match item {
            Item::Open => saved.push(state.cwd.clone()),
            Item::Close => state.cwd = saved.pop().unwrap_or_else(|| state.cwd.clone()),
            Item::Command(simple, join) => simple_command(context, &mut state, simple, *join)?,
        }
    }
    Ok(())
}

fn simple_command(context: &Context, state: &mut State, simple: &Simple, join: Join) -> Verdict {
    for (name, value) in &simple.assignments {
        tools::assignment(context, name, value)?;
        if name == "CDPATH" {
            state.cwd = None;
        }
    }
    for redirect in simple.redirects.iter().filter(|r| r.write) {
        context.write(state.cwd.as_deref(), &redirect.target, Access::Put)?;
    }
    let words = strip_keywords(&simple.words);
    if words.is_empty() {
        return Ok(());
    }
    dispatch(context, state, words, join)
}

fn strip_keywords(words: &[Word]) -> &[Word] {
    let mut words = words;
    while let Some(first) = words.first() {
        let keyword = first.plain()
            && matches!(
                first.text.as_str(),
                "!" | "{"
                    | "}"
                    | "if"
                    | "then"
                    | "else"
                    | "elif"
                    | "while"
                    | "until"
                    | "do"
                    | "fi"
                    | "done"
                    | "esac"
            );
        if !keyword {
            break;
        }
        words = &words[1..];
    }
    words
}

pub(crate) fn dispatch(
    context: &Context,
    state: &mut State,
    words: &[Word],
    join: Join,
) -> Verdict {
    let agent = context.role.is_agent();
    let head = &words[0];
    if !head.plain() {
        return if agent {
            Err(format!(
                "the command name `{}` is computed at run time; write it plainly",
                head.text
            ))
        } else {
            Ok(())
        };
    }
    let name = head.text.as_str();
    let base = name.rsplit('/').next().unwrap_or(name);
    if agent && name.contains('/') && !is_system_path(name) {
        return Err(format!(
            "agents do not run scripts or binaries by path (`{name}`); use a Make target or ask the main session"
        ));
    }
    let rest = &words[1..];
    match base {
        "sudo" | "doas" | "su" | "pkexec" | "run0" => Err(
            "privileged commands are never run by a Claude session; ask the person to run it in their own terminal".into(),
        ),
        "env" => env(context, state, rest, join),
        "nice" | "nohup" | "time" | "timeout" | "stdbuf" | "ionice" | "exec" | "builtin"
        | "command" => wrapped(context, state, base, rest, join),
        "xargs" => xargs(context, state, rest, join),
        "sh" | "bash" | "zsh" | "dash" | "ksh" => shell_wrapper(context, state, base, rest),
        "eval" if agent => Err("agents do not use `eval`; write the command plainly".into()),
        "eval" => line(context, &joined(rest), state.cwd.clone(), state.depth + 1),
        "source" | "." if agent => Err("agents do not source scripts; run a Make target".into()),
        "watch" => {
            let spec = Spec { short: "n", long: &["interval"], stop_at_positional: true };
            let at = args::split(rest, &spec).stop;
            line(context, &joined(&rest[at..]), state.cwd.clone(), state.depth + 1)
        }
        "cd" | "pushd" => {
            change_directory(context, state, rest, join);
            Ok(())
        }
        "popd" => {
            state.cwd = None;
            Ok(())
        }
        "export" | "declare" | "typeset" | "local" | "readonly" => {
            for word in rest.iter().filter(|w| !w.text.starts_with('-')) {
                if let Some((name, value)) = word.text.split_once('=') {
                    let value = word.with_text(value.to_owned());
                    tools::assignment(context, name, &value)?;
                }
            }
            Ok(())
        }
        "alias" | "trap" | "function" | "case" | "select" | "coproc" | "enable" | "shopt"
            if agent =>
        {
            Err(format!("agents do not use `{base}`; write plain commands"))
        }
        "git" => git::git(context, state, rest),
        "gh" => github::gh(context, rest),
        "make" | "gmake" => dispatch::make(context, rest),
        "docker" | "podman" | "docker-compose" => dispatch::docker(context, base, rest),
        _ => tools::other(context, state, base, rest),
    }
}

fn is_system_path(name: &str) -> bool {
    ["/usr/bin/", "/bin/", "/usr/local/bin/"]
        .iter()
        .any(|prefix| {
            name.strip_prefix(prefix)
                .is_some_and(|rest| !rest.contains('/'))
        })
}

pub(crate) fn joined(words: &[Word]) -> String {
    words
        .iter()
        .map(|w| shell_quote(&w.text))
        .collect::<Vec<_>>()
        .join(" ")
}

fn shell_quote(text: &str) -> String {
    if !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./=:@%+,".contains(c))
    {
        text.to_owned()
    } else {
        format!("'{}'", text.replace('\'', "'\\''"))
    }
}

fn change_directory(context: &Context, state: &mut State, rest: &[Word], join: Join) {
    let target = rest
        .iter()
        .find(|w| !matches!(w.text.as_str(), "-L" | "-P" | "-e" | "-@"));
    state.cwd = match (join, target) {
        (Join::Pipe | Join::Background, _) => None,
        (_, None) => context.home.clone(),
        (_, Some(word)) if !word.plain() || word.text == "-" => None,
        (_, Some(word)) => {
            let path = paths::expand_home(&word.text, context.home.as_deref());
            if path.is_absolute() {
                Some(paths::lexical(&path))
            } else {
                state
                    .cwd
                    .as_ref()
                    .map(|cwd| paths::lexical(&cwd.join(path)))
            }
        }
    };
}

fn env(context: &Context, state: &mut State, rest: &[Word], join: Join) -> Verdict {
    let mut index = 0;
    while let Some(word) = rest.get(index) {
        let text = word.text.as_str();
        match text {
            "-i" | "--ignore-environment" | "-0" | "--null" | "-v" | "--debug" | "-" => index += 1,
            "-u" | "--unset" => index += 2,
            "--" => {
                index += 1;
                break;
            }
            "-C" | "--chdir" => {
                if context.role.is_agent() {
                    return Err("agents change directory with `cd`, not `env -C`".into());
                }
                state.cwd = None;
                index += 2;
            }
            "-S" | "--split-string" => {
                let inner = rest
                    .get(index + 1)
                    .map(|w| w.text.clone())
                    .unwrap_or_default();
                return line(context, &inner, state.cwd.clone(), state.depth + 1);
            }
            _ if text.starts_with("--unset=") || text.starts_with("--chdir=") => {
                if text.starts_with("--chdir=") {
                    state.cwd = None;
                }
                index += 1;
            }
            _ if text.starts_with('-') && context.role.is_agent() => {
                return Err(format!("unknown `env` option `{text}`"));
            }
            _ if text.starts_with('-') => index += 1,
            _ => match text.split_once('=') {
                Some((name, value)) if shell::valid_name(name) => {
                    let value = word.with_text(value.to_owned());
                    tools::assignment(context, name, &value)?;
                    index += 1;
                }
                _ => break,
            },
        }
    }
    match rest.get(index..) {
        Some(inner) if !inner.is_empty() => dispatch(context, state, inner, join),
        _ => Ok(()),
    }
}

fn wrapped(context: &Context, state: &mut State, base: &str, rest: &[Word], join: Join) -> Verdict {
    let spec = match base {
        "timeout" => Spec {
            short: "sk",
            long: &["signal", "kill-after"],
            stop_at_positional: true,
        },
        "nice" => Spec {
            short: "n",
            long: &["adjustment"],
            stop_at_positional: true,
        },
        "stdbuf" => Spec {
            short: "ioe",
            long: &["input", "output", "error"],
            stop_at_positional: true,
        },
        "ionice" => Spec {
            short: "cnpPu",
            long: &["class", "classdata", "pid", "pgid", "uid"],
            stop_at_positional: true,
        },
        "exec" => Spec {
            short: "a",
            long: &[],
            stop_at_positional: true,
        },
        _ => Spec {
            short: "",
            long: &[],
            stop_at_positional: true,
        },
    };
    let parsed = args::split(rest, &spec);
    let mut at = parsed.stop;
    if base == "command" && parsed.has(&["-v", "-V"]) {
        return Ok(());
    }
    if base == "timeout" {
        at += 1;
    }
    match rest.get(at..) {
        Some(inner) if !inner.is_empty() => dispatch(context, state, inner, join),
        _ => Ok(()),
    }
}

fn shell_wrapper(context: &Context, state: &State, base: &str, rest: &[Word]) -> Verdict {
    let agent = context.role.is_agent();
    let mut index = 0;
    while let Some(word) = rest.get(index) {
        let text = word.text.as_str();
        if text == "-o" || text == "+o" || text == "-O" || text == "+O" {
            index += 2;
            continue;
        }
        if (text.starts_with('-') || text.starts_with('+')) && text.len() > 1 && text != "--" {
            if !text.starts_with("--") && text.contains('c') {
                return match rest.get(index + 1) {
                    Some(command) if command.plain() || !agent => {
                        line(context, &command.text, state.cwd.clone(), state.depth + 1)
                    }
                    Some(_) => Err(format!(
                        "the script given to `{base} -c` is computed at run time; write it plainly"
                    )),
                    None => Err(format!("`{base} -c` without a command")),
                };
            }
            index += 1;
            continue;
        }
        break;
    }
    if agent {
        Err(format!(
            "agents run `{base}` only as `{base} -c '<plain commands>'`; scripts and standard input are opaque"
        ))
    } else {
        Ok(())
    }
}

fn xargs(context: &Context, state: &mut State, rest: &[Word], join: Join) -> Verdict {
    let spec = Spec {
        short: "nLPsdEIaei",
        long: &[
            "max-args",
            "max-lines",
            "max-procs",
            "max-chars",
            "delimiter",
            "eof",
            "replace",
            "arg-file",
            "process-slot-var",
        ],
        stop_at_positional: true,
    };
    let at = args::split(rest, &spec).stop;
    let inner = &rest[at..];
    if inner.is_empty() {
        return Ok(());
    }
    if context.role.is_agent() && !tools::reads_only(inner) {
        return Err(format!(
            "`xargs {}` hides which paths it changes; list the paths explicitly",
            inner[0].text
        ));
    }
    dispatch(context, state, inner, join)
}
