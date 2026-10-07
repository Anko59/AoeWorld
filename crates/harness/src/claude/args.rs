//! Option splitting shared by the command rules: short clusters (`-am`),
//! attached values (`-oFILE`, `--output=FILE`) and long-option abbreviations
//! (`--outp` for `--output`), which tools such as git and curl accept.
use super::shell::Word;

#[derive(Debug)]
pub(crate) struct Opt {
    /// `-x` for a short option, `--name` (canonical when abbreviated) for a long one.
    pub(crate) name: String,
    pub(crate) value: Option<Word>,
}

#[derive(Debug, Default)]
pub(crate) struct Args<'a> {
    pub(crate) options: Vec<Opt>,
    pub(crate) positionals: Vec<&'a Word>,
    /// Index of the first word not parsed (the inner command for wrappers).
    pub(crate) stop: usize,
}

pub(crate) struct Spec<'s> {
    /// Short options that take a value.
    pub(crate) short: &'s str,
    /// Long options that take a value.
    pub(crate) long: &'s [&'s str],
    /// Stop option parsing at the first positional (`git`, `xargs`, `env`).
    pub(crate) stop_at_positional: bool,
}

pub(crate) fn split<'a>(args: &'a [Word], spec: &Spec<'_>) -> Args<'a> {
    let mut out = Args::default();
    let mut index = 0;
    let mut literal = false;
    while index < args.len() {
        let word = &args[index];
        let text = word.text.as_str();
        index += 1;
        if literal || text == "-" || !text.starts_with('-') {
            out.positionals.push(word);
            if spec.stop_at_positional && !literal {
                out.stop = index - 1;
                return out;
            }
            continue;
        }
        if text == "--" {
            literal = true;
            if spec.stop_at_positional {
                out.stop = index;
                return out;
            }
            continue;
        }
        if let Some(long) = text.strip_prefix("--") {
            let (name, attached) = match long.split_once('=') {
                Some((name, value)) => (name, Some(value)),
                None => (long, None),
            };
            let canonical = canonical(name, spec.long);
            let value = match (attached, canonical) {
                (Some(value), _) => Some(word.with_text(value.to_owned())),
                (None, Some(_)) => {
                    index += 1;
                    args.get(index - 1).cloned()
                }
                (None, None) => None,
            };
            out.options.push(Opt {
                name: format!("--{}", canonical.unwrap_or(name)),
                value,
            });
            continue;
        }
        let cluster: Vec<char> = text[1..].chars().collect();
        for (at, flag) in cluster.iter().enumerate() {
            if spec.short.contains(*flag) {
                let rest: String = cluster[at + 1..].iter().collect();
                let value = if rest.is_empty() {
                    index += 1;
                    args.get(index - 1).cloned()
                } else {
                    Some(word.with_text(rest))
                };
                out.options.push(Opt {
                    name: format!("-{flag}"),
                    value,
                });
                break;
            }
            out.options.push(Opt {
                name: format!("-{flag}"),
                value: None,
            });
        }
    }
    out.stop = args.len();
    out
}

/// Exact long name, else the first value-taking name it abbreviates.
fn canonical<'s>(name: &str, long: &[&'s str]) -> Option<&'s str> {
    long.iter()
        .find(|candidate| **candidate == name)
        .or_else(|| {
            long.iter()
                .find(|candidate| name.len() >= 3 && candidate.starts_with(name))
        })
        .copied()
}

impl Args<'_> {
    pub(crate) fn has(&self, names: &[&str]) -> bool {
        self.options
            .iter()
            .any(|o| names.contains(&o.name.as_str()))
    }

    /// A long option or an abbreviation of it at least `min` characters long.
    pub(crate) fn has_long(&self, full: &str, min: usize) -> bool {
        self.options.iter().any(|o| {
            o.name.len() >= min && full.starts_with(o.name.as_str()) && o.name.starts_with("--")
        })
    }

    pub(crate) fn values(&self, names: &[&str]) -> impl Iterator<Item = &Word> {
        self.options
            .iter()
            .filter(move |o| names.contains(&o.name.as_str()))
            .filter_map(|o| o.value.as_ref())
    }
}

/// The text of each positional, for rules that match verbs.
pub(crate) fn texts<'a>(words: &[&'a Word]) -> Vec<&'a str> {
    words.iter().map(|w| w.text.as_str()).collect()
}
