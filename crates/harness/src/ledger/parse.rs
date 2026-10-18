//! Reading the model facts out of pull request text: `Co-Authored-By`
//! trailers, the body's "Written by" line and the review badge `make ship`
//! renders. Pure functions over strings, so fixtures can pin every shape.
use std::collections::BTreeSet;

/// One rendered review badge, for example
/// `🟡 **Review: 8/10** (high tier · 5 reviewers · 2 rounds · claude-sonnet-5-5 high): 4 confirmed · 0 disputed · 0 refuted`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Badge {
    pub(super) closing: bool,
    pub(super) grade: u8,
    pub(super) tier: String,
    pub(super) reviewers: u32,
    pub(super) rounds: u32,
    pub(super) model: String,
    pub(super) effort: String,
    pub(super) confirmed: u32,
}

/// The distinct badges of a body, in order. The description repeats the final
/// badge under "How" and under "Adversarial review"; the first one stands.
pub(super) fn badges(body: &str) -> Vec<Badge> {
    let mut found: Vec<Badge> = Vec::new();
    for line in body.lines() {
        if let Some(badge) = badge(line)
            && !found.contains(&badge)
        {
            found.push(badge);
        }
    }
    found
}

fn badge(line: &str) -> Option<Badge> {
    let (closing, rest) = if let Some((_, rest)) = line.split_once("**Closing review: ") {
        (true, rest)
    } else {
        (false, line.split_once("**Review: ")?.1)
    };
    let (grade, rest) = rest.split_once("/10**")?;
    let grade = grade.trim().parse().ok()?;
    let (inside, after) = rest.trim_start().strip_prefix('(')?.split_once(')')?;
    let parts: Vec<&str> = inside.split('·').map(str::trim).collect();
    let [tier, reviewers, rounds, model] = parts.as_slice() else {
        return None;
    };
    let (model, effort) = model.split_once(' ').unwrap_or((model, ""));
    let confirmed = after
        .split('·')
        .find_map(|part| {
            part.trim()
                .trim_start_matches(':')
                .trim()
                .strip_suffix(" confirmed")
        })
        .and_then(|n| n.trim().parse().ok())?;
    Some(Badge {
        closing,
        grade,
        tier: tier.strip_suffix(" tier")?.to_owned(),
        reviewers: leading_number(reviewers)?,
        rounds: leading_number(rounds)?,
        model: canonical(model),
        effort: effort.to_owned(),
        confirmed,
    })
}

fn leading_number(text: &str) -> Option<u32> {
    text.split_whitespace().next()?.parse().ok()
}

/// The grade a closing review's branch was approved at first: the
/// `approved 8/10` its review line ends with.
pub(super) fn approved_grade(body: &str) -> Option<u8> {
    body.lines().find_map(|line| {
        let tail = line.rsplit_once("approved ")?.1;
        let (grade, _) = tail.split_once("/10")?;
        grade.trim().parse().ok()
    })
}

/// Every model named on the body's "Written by" lines, first named first.
pub(super) fn written_by(body: &str) -> Vec<String> {
    let mut models = Vec::new();
    for line in body.lines() {
        let line = line.trim_start_matches(['-', '*', ' ']).trim();
        if line.starts_with("Written by") {
            push_all(&mut models, model_names(line));
        }
    }
    models
}

/// The models credited by `Co-Authored-By` trailers; bots are not models.
pub(super) fn trailer_models(message: &str) -> Vec<String> {
    let mut models = Vec::new();
    for line in message.lines() {
        if let Some((key, value)) = line.split_once(':')
            && key.trim().eq_ignore_ascii_case("co-authored-by")
        {
            push_all(&mut models, model_names(value));
        }
    }
    models
}

fn push_all(into: &mut Vec<String>, names: Vec<String>) {
    for name in names {
        if !into.contains(&name) {
            into.push(name);
        }
    }
}

/// Model names in prose: `Claude Opus 5.5`, `GPT-6.1 Sol (Codex)`, `GLM 5.3 flash`,
/// `DeepSeek V4`, in the canonical spelling.
pub(super) fn model_names(text: &str) -> Vec<String> {
    let words: Vec<&str> = text
        .split(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | '(' | ')' | '<' | '>'))
        .filter(|word| !word.is_empty())
        .collect();
    let mut names = Vec::new();
    for (i, word) in words.iter().enumerate() {
        let next = words.get(i + 1);
        let name = match *word {
            "Claude" => match (next, words.get(i + 2)) {
                (Some(family), Some(v)) if is_version(v) => {
                    Some(format!("Claude {family} {}", v.trim_end_matches('.')))
                }
                _ => None,
            },
            "GLM" => next.filter(|v| is_version(v)).map(|v| {
                match words
                    .get(i + 2)
                    .filter(|w| matches!(**w, "flash" | "air" | "turbo"))
                {
                    Some(size) => format!("GLM {v} {size}"),
                    None => format!("GLM {v}"),
                }
            }),
            "DeepSeek" => next.map(|v| format!("DeepSeek {v}")),
            // Codex's trailer spells it `GPT 6.1 sol`: the same model as `GPT-6.1 Sol`.
            "GPT" => next.filter(|v| is_version(v)).map(|v| {
                let tier = words
                    .get(i + 2)
                    .filter(|t| t.chars().all(char::is_alphabetic))
                    .map(|t| {
                        let mut chars = t.chars();
                        chars.next().map_or_else(String::new, |c| {
                            c.to_uppercase()
                                .chain(chars.map(|c| c.to_ascii_lowercase()))
                                .collect()
                        })
                    });
                tier.map_or_else(|| format!("GPT-{v}"), |t| format!("GPT-{v} {t}"))
            }),
            w if w.starts_with("GPT-") => {
                let tier = next.filter(|n| {
                    n.chars().next().is_some_and(char::is_uppercase)
                        && n.chars().all(char::is_alphabetic)
                });
                Some(tier.map_or_else(|| (*w).to_owned(), |t| format!("{w} {t}")))
            }
            _ => None,
        };
        if let Some(name) = name
            && !names.contains(&name)
        {
            names.push(name);
        }
    }
    names
}

fn is_version(word: &str) -> bool {
    !word.is_empty() && word.chars().all(|c| c.is_ascii_digit() || c == '.')
}

/// A review model id in the same spelling as a person writes it:
/// `claude-sonnet-5-5` is `Claude Sonnet 5.5`, `gpt-6.1-sol` is `GPT-6.1 Sol`.
pub(super) fn canonical(id: &str) -> String {
    let title = |s: &str| {
        let mut chars = s.chars();
        chars
            .next()
            .map_or_else(String::new, |c| c.to_uppercase().chain(chars).collect())
    };
    let parts: Vec<&str> = id.split('-').collect();
    match parts.as_slice() {
        ["claude", family, major, minor] => format!("Claude {} {major}.{minor}", title(family)),
        ["gpt", version, tier] => format!("GPT-{version} {}", title(tier)),
        _ => id.to_owned(),
    }
}

/// Whether the pull request says it is one part of a larger change.
pub(super) fn is_split(title: &str, body: &str) -> bool {
    let lower = |s: &str| s.to_ascii_lowercase();
    let (title, body) = (lower(title), lower(body));
    title.starts_with("split")
        || [
            "split from #",
            "split out of #",
            "split of #",
            "split off from #",
        ]
        .iter()
        .any(|p| body.contains(p))
}

/// The pull requests a `Revert "…(#N)"` commit on dev undid, from
/// `git log --format=%s` lines.
pub(super) fn reverted_numbers(subjects: &str) -> BTreeSet<u64> {
    subjects
        .lines()
        .filter_map(|line| line.strip_prefix("Revert \""))
        .filter_map(|quoted| {
            // Only inside the quoted original subject: a squash-merged revert PR appends its
            // own `(#N)` after the closing quote.
            let quoted = quoted.rsplit_once('"').map_or(quoted, |(inner, _)| inner);
            let after = quoted.rsplit_once("(#")?.1;
            after.split_once(')')?.0.parse().ok()
        })
        .collect()
}
