//! Protected Make variables, and the rule that agents do not program the
//! shell: no builtin that creates, changes or exports shell state. The lexer
//! refuses arithmetic, every parameter expansion but the plain forms,
//! here-documents, here-strings and backslash-newline continuations
//! (`Parsed::opaque`), and `bash::simple_command` refuses assignments that
//! stand alone. Every rule here binds agent roles only.
use super::tools;
use crate::agents::{
    context::{Context, Verdict},
    shell::{Word, valid_name},
};

/// Variable names that can change what a Make target executes. Agents are
/// also refused `GNUMAKEFLAGS` and `MFLAGS`, which the main session keeps.
pub(crate) fn dangerous_make_variable(name: &str, agent: bool) -> bool {
    matches!(
        name,
        "SHELL" | "MAKEFLAGS" | "MAKEFILES" | "ROOT" | "UID" | "GID" | ".SHELLFLAGS"
    ) || (agent && matches!(name, "GNUMAKEFLAGS" | "MFLAGS"))
        || name.ends_with("_IMAGE")
        || name.ends_with("_RUN")
        || name.ends_with("_MOUNTS")
        || name.ends_with("_MOUNT")
        || name.starts_with("GIT_")
        || name.ends_with("_REAL")
}

/// Builtins and keywords that assign, export, unset or reinterpret shell
/// variables, options, functions, aliases, traps, command lookup or, for
/// `exec`, the shell's own descriptors and process.
const STATEFUL: &[&str] = &[
    "exec",
    "export",
    "declare",
    "typeset",
    "local",
    "readonly",
    "unset",
    "read",
    "mapfile",
    "readarray",
    "getopts",
    "let",
    "set",
    "shopt",
    "enable",
    "eval",
    "source",
    ".",
    "alias",
    "unalias",
    "trap",
    "function",
    "select",
    "coproc",
    "hash",
    "fc",
    "history",
    "bind",
    "complete",
    "compgen",
    "compopt",
    "ulimit",
    "umask",
    "shift",
    "disown",
];

/// Refuse agent commands that change shell state. `base` is the plain
/// command name; `args` are the words after it.
pub(crate) fn shell_state(context: &Context, base: &str, args: &[Word]) -> Verdict {
    if !context.role.is_agent() {
        return Ok(());
    }
    if STATEFUL.contains(&base) {
        return Err(format!(
            "agents do not program the shell (`{base}` changes shell state); run plain commands, prefixing `NAME=value` where a command needs a variable"
        ));
    }
    match base {
        "printf" => printf(args),
        "wait" if args.iter().any(|word| word.text.starts_with('-')) => {
            Err("agents use `wait` without options (`-p` assigns a variable)".into())
        }
        "for" => match args.first() {
            Some(name) if name.plain() && valid_name(&name.text) => {
                tools::assignment(context, &name.text, &Word::literal("value"))
            }
            _ => Err("agents loop only over a literal, unprotected variable name".into()),
        },
        _ => Ok(()),
    }
}

/// `printf` without options (`-v` assigns) and with a literal format that
/// has no `%n` conversion (it assigns the count to an argument's name).
fn printf(args: &[Word]) -> Verdict {
    let args = match args.first() {
        Some(word) if word.text == "--" => &args[1..],
        _ => args,
    };
    let Some(format) = args.first() else {
        return Ok(());
    };
    if !format.plain() || (format.text.starts_with('-') && format.text.len() > 1) {
        return Err("agents use `printf` with a literal format and no options".into());
    }
    let mut rest = format.text.as_str();
    while let Some(at) = rest.find('%') {
        rest = &rest[at + 1..];
        if let Some(after) = rest.strip_prefix('%') {
            rest = after;
            continue;
        }
        let conversion = rest.trim_start_matches(|c: char| "#'-+ 0123456789.*hjlLqtz".contains(c));
        if conversion.starts_with('n') {
            return Err("agents do not use printf's `%n` (it assigns a variable)".into());
        }
    }
    Ok(())
}
