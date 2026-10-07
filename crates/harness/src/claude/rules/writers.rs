//! Which arguments a file-writing command writes to. Every role is judged, so
//! the main session cannot clobber Git metadata or the harness records by
//! mistake; agents are further held to their role's files.
use crate::claude::{
    args::{self, Spec},
    context::{Access, Context, Verdict},
    shell::Word,
};
use std::path::Path;

pub(crate) fn judge(context: &Context, cwd: Option<&Path>, base: &str, rest: &[Word]) -> Verdict {
    let put = |word: &Word| context.write(cwd, word, Access::Put);
    let remove = |word: &Word| context.write(cwd, word, Access::Remove);
    let split = |short: &str, long: &[&str]| {
        args::split(
            rest,
            &Spec {
                short,
                long,
                stop_at_positional: false,
            },
        )
    };
    match base {
        "rm" | "rmdir" | "unlink" | "shred" => {
            split("", &[]).positionals.into_iter().try_for_each(remove)
        }
        "touch" | "mkdir" | "mkfifo" | "tee" => split("dtrm", &["date", "reference", "mode"])
            .positionals
            .into_iter()
            .try_for_each(put),
        "truncate" => split("sr", &["size", "reference"])
            .positionals
            .into_iter()
            .try_for_each(put),
        "chmod" | "chown" | "chgrp" => {
            let parsed = split("", &["reference"]);
            // `chmod -x f` spells the mode as an option; then no positional is the mode.
            let mode_option = rest.iter().any(|w| {
                w.text.len() > 1
                    && w.text.starts_with('-')
                    && w.text[1..].chars().all(|c| "rwxXst".contains(c))
            });
            let skip = usize::from(!parsed.has(&["--reference"]) && !mode_option);
            parsed.positionals.into_iter().skip(skip).try_for_each(put)
        }
        "cp" | "install" | "ln" => copy(context, cwd, base, rest),
        "mv" => {
            let parsed = split("tS", &["target-directory", "suffix"]);
            if let Some(target) = parsed.values(&["-t", "--target-directory"]).next() {
                put(target)?;
            }
            parsed.positionals.into_iter().try_for_each(remove)
        }
        "dd" => rest
            .iter()
            .filter_map(|w| {
                w.text
                    .strip_prefix("of=")
                    .map(|path| w.with_text(path.to_owned()))
            })
            .try_for_each(|w| put(&w)),
        "sort" => split(
            "oTkStz",
            &[
                "output",
                "temporary-directory",
                "key",
                "buffer-size",
                "field-separator",
            ],
        )
        .values(&["-o", "--output"])
        .try_for_each(put),
        "sed" => sed(context, cwd, rest),
        "awk" | "gawk" | "mawk" | "nawk" => awk(context, cwd, rest),
        "yq" => {
            let parsed = split("", &[]);
            if parsed.has(&["-i", "--inplace"]) {
                parsed.positionals.into_iter().skip(1).try_for_each(put)?;
            }
            Ok(())
        }
        "tar" => tar(context, cwd, rest),
        "unzip" => {
            let parsed = split("dxP", &[]);
            if parsed.has(&["-l", "-t", "-v", "-Z"]) {
                return Ok(());
            }
            match parsed.values(&["-d"]).next() {
                Some(directory) => context.write(cwd, directory, Access::Tree),
                None => context.write(cwd, &Word::literal("."), Access::Tree),
            }
        }
        "mktemp" => split("p", &["tmpdir", "suffix"])
            .values(&["-p", "--tmpdir"])
            .try_for_each(put),
        "curl" => super::network::curl(context, cwd, rest),
        "wget" => super::network::wget(context, cwd, rest),
        "patch" => split(
            "oiprdDBFzY",
            &["output", "input", "reject-file", "directory"],
        )
        .values(&["-o", "--output", "-r", "--reject-file"])
        .try_for_each(put),
        _ => Ok(()),
    }
}

fn copy(context: &Context, cwd: Option<&Path>, base: &str, rest: &[Word]) -> Verdict {
    let parsed = args::split(
        rest,
        &Spec {
            short: "tSmogb",
            long: &[
                "target-directory",
                "suffix",
                "mode",
                "owner",
                "group",
                "backup",
            ],
            stop_at_positional: false,
        },
    );
    let recursive = parsed.has(&["-r", "-R", "-a", "--recursive", "--archive"]);
    let mut sources = parsed.positionals.clone();
    let target = match parsed.values(&["-t", "--target-directory"]).next() {
        Some(target) => target.clone(),
        None => match sources.pop() {
            Some(target) => target.clone(),
            None => return Ok(()),
        },
    };
    if base == "ln" && sources.is_empty() {
        return Ok(());
    }
    let access = if recursive { Access::Tree } else { Access::Put };
    let into_directory = context
        .resolve(cwd, &target)?
        .is_some_and(|landing| landing.is_dir());
    if !into_directory {
        return context.write(cwd, &target, access);
    }
    for source in sources {
        let name = source
            .text
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or_default();
        let landing = target.with_text(format!("{}/{name}", target.text.trim_end_matches('/')));
        context.write(cwd, &landing, access)?;
    }
    Ok(())
}

fn sed(context: &Context, cwd: Option<&Path>, rest: &[Word]) -> Verdict {
    let mut in_place = false;
    let mut program: Option<&Word> = None;
    let mut files = Vec::new();
    let mut index = 0;
    while let Some(word) = rest.get(index) {
        index += 1;
        let text = word.text.as_str();
        match text {
            "-i" | "--in-place" => {
                in_place = true;
                // BSD takes the next word as the backup suffix: `sed -i '' …`.
                if rest
                    .get(index)
                    .is_some_and(|w| w.text.is_empty() || w.text.starts_with('.'))
                {
                    index += 1;
                }
            }
            "-e" | "--expression" => {
                program = rest.get(index);
                index += 1;
            }
            "-f" | "--file" => {
                if context.role.is_agent() {
                    return Err("sed scripts from files are opaque; give the program inline".into());
                }
                program = Some(word);
                index += 1;
            }
            "-l" | "--line-length" => index += 1,
            _ if text.starts_with("--in-place=") || text.starts_with("--expression=") => {
                in_place |= text.starts_with("--in-place=");
                if program.is_none() && text.starts_with("--expression=") {
                    program = Some(word);
                }
            }
            _ if text.starts_with('-') && text.len() > 1 && !text.starts_with("--") => {
                in_place |= text.contains('i');
                if text.ends_with('e') || text.ends_with('f') {
                    program = rest.get(index);
                    index += 1;
                }
            }
            _ if text.starts_with("--") => {}
            _ if program.is_none() => program = Some(word),
            _ => files.push(word),
        }
    }
    if context.role.is_agent() && program.is_some_and(|p| sed_program_writes(&p.text)) {
        return Err("sed `w`, `W` and `e` commands write or run things; use the Edit tool".into());
    }
    if in_place {
        files
            .into_iter()
            .try_for_each(|file| context.write(cwd, file, Access::Put))?;
    }
    Ok(())
}

/// Conservative scan of a sed program for commands that write or execute.
pub(crate) fn sed_program_writes(program: &str) -> bool {
    let chars: Vec<char> = program.chars().collect();
    let mut at = 0;
    let skip_delimited = |at: &mut usize, parts: usize| {
        let Some(&delimiter) = chars.get(*at) else {
            return;
        };
        *at += 1;
        let mut seen = 0;
        while *at < chars.len() && seen < parts {
            match chars[*at] {
                '\\' => *at += 1,
                c if c == delimiter => seen += 1,
                _ => {}
            }
            *at += 1;
        }
    };
    while at < chars.len() {
        let c = chars[at];
        match c {
            ' ' | '\t' | '\n' | ';' | '{' | '}' | '!' | ',' | '$' => at += 1,
            '0'..='9' | '~' | '+' => at += 1,
            '/' => skip_delimited(&mut at, 1),
            '\\' => {
                at += 1;
                skip_delimited(&mut at, 1);
            }
            's' | 'y' => {
                at += 1;
                skip_delimited(&mut at, 2);
                while at < chars.len() && !matches!(chars[at], ';' | '\n' | '}') {
                    if c == 's' && matches!(chars[at], 'w' | 'W' | 'e') {
                        return true;
                    }
                    at += 1;
                }
            }
            'w' | 'W' | 'e' => return true,
            'r' | 'R' | 'a' | 'i' | 'c' | ':' | 'b' | 't' | 'T' => {
                while at < chars.len()
                    && chars[at] != '\n'
                    && !(matches!(c, 'b' | 't' | 'T' | ':') && chars[at] == ';')
                {
                    at += 1;
                }
            }
            'p' | 'P' | 'd' | 'D' | 'n' | 'N' | 'q' | 'Q' | 'l' | '=' | 'g' | 'G' | 'h' | 'H'
            | 'x' | 'z' | 'F' => at += 1,
            _ => return true,
        }
    }
    false
}

fn awk(context: &Context, cwd: Option<&Path>, rest: &[Word]) -> Verdict {
    let parsed = args::split(
        rest,
        &Spec {
            short: "fvFiE",
            long: &["file", "assign", "field-separator", "include", "exec"],
            stop_at_positional: false,
        },
    );
    let in_place = parsed
        .values(&["-i", "--include"])
        .any(|w| w.text.contains("inplace"));
    if context.role.is_agent() {
        if parsed.has(&["-f", "--file", "-E", "--exec"]) {
            return Err("awk programs from files are opaque; give the program inline".into());
        }
        let program = parsed
            .positionals
            .first()
            .map(|w| w.text.as_str())
            .unwrap_or_default();
        let redirect = program.match_indices('>').any(|(at, _)| {
            let after = program[at + 1..].trim_start();
            after.starts_with('"') || after.starts_with('>')
        });
        if redirect
            || program.contains("system")
            || program.contains("|&")
            || program.replace("||", "").contains('|')
        {
            return Err("awk output redirection, pipes and `system()` write or run things; use the Edit tool".into());
        }
    }
    if in_place {
        parsed
            .positionals
            .into_iter()
            .skip(1)
            .try_for_each(|w| context.write(cwd, w, Access::Put))?;
    }
    Ok(())
}

fn tar(context: &Context, cwd: Option<&Path>, rest: &[Word]) -> Verdict {
    let mut words: Vec<Word> = rest.to_vec();
    // Old style: `tar xzf archive.tgz` → `-xzf archive.tgz`.
    if let Some(first) = words.first_mut().filter(|w| !w.text.starts_with('-')) {
        first.text.insert(0, '-');
    }
    let parsed = args::split(
        &words,
        &Spec {
            short: "fCbTXgKN",
            long: &[
                "file",
                "directory",
                "files-from",
                "exclude-from",
                "listed-incremental",
                "starting-file",
                "newer",
            ],
            stop_at_positional: false,
        },
    );
    let extract = parsed.has(&["-x", "--extract", "--get"]);
    let create = parsed.has(&[
        "-c",
        "-r",
        "-u",
        "-A",
        "--create",
        "--append",
        "--update",
        "--catenate",
        "--concatenate",
    ]);
    if extract {
        let directory = parsed
            .values(&["-C", "--directory"])
            .next()
            .cloned()
            .unwrap_or_else(|| Word::literal("."));
        context.write(cwd, &directory, Access::Tree)?;
    }
    if create {
        parsed
            .values(&["-f", "--file"])
            .filter(|w| w.text != "-")
            .try_for_each(|w| context.write(cwd, w, Access::Put))?;
    }
    Ok(())
}
