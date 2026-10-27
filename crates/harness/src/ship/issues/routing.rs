//! Numbered workstream destinations and fingerprinted body sections.
use super::{BODY_LIMIT, Result, Target, validate_text};
use serde::Deserialize;
use std::{collections::HashSet, path::Path};

const CONFIG: &str = "gates/issues.json";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Config {
    pub(super) game_review: u64,
    pub(super) other_review: u64,
    pub(super) nightly: u64,
}

impl Config {
    pub(super) fn load(root: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(root.join(CONFIG))
            .map_err(|error| format!("{CONFIG}: {error}"))?;
        let config: Self =
            serde_json::from_str(&raw).map_err(|error| format!("{CONFIG}: {error}"))?;
        if [config.game_review, config.other_review, config.nightly].contains(&0) {
            return Err(format!("{CONFIG}: issue numbers must be positive").into());
        }
        if config.game_review == config.other_review {
            return Err(format!("{CONFIG}: review destinations must differ").into());
        }
        Ok(config)
    }

    pub(super) fn review(&self, path: &str) -> u64 {
        let path = path.strip_prefix("./").unwrap_or(path);
        let game = [
            "crates/core/",
            "crates/scenario/",
            "crates/protocol/",
            "crates/geodata/",
            "crates/simulation/",
            "crates/server/",
            "crates/client/",
            "crates/rendering/",
            "crates/map/",
            "crates/assets/",
            "crates/sdk/",
            "crates/sdk-python/",
            "ai/",
            "browser/src/",
            "web/",
        ];
        if !path.ends_with(".md")
            && !path.starts_with("web/dev/")
            && game.iter().any(|prefix| path.starts_with(prefix))
        {
            self.game_review
        } else {
            self.other_review
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Snapshot {
    state: String,
    body: String,
    updated_at: String,
}

/// Update only the section identified by a stable fingerprint. Retry on a
/// concurrent change; GitHub does not offer an atomic compare-and-set.
pub(super) fn update(
    target: &Target,
    root: &Path,
    number: u64,
    key: &str,
    section: &str,
    game: bool,
) -> Result<()> {
    let number = number.to_string();
    for _ in 0..3 {
        let read = || {
            target.gh(
                root,
                &["issue", "view", &number, "--json", "state,body,updatedAt"],
                None,
            )
        };
        let current: Snapshot = serde_json::from_str(&read()?)?;
        if current.state != "OPEN" {
            return Err(
                format!("{CONFIG}: configured issue #{number} is {}", current.state).into(),
            );
        }
        let body = splice(&current.body, key, section, game)?;
        validate_text("ISSUE_BODY", &body, BODY_LIMIT)?;
        let latest: Snapshot = serde_json::from_str(&read()?)?;
        if latest.state != "OPEN" {
            return Err(format!("{CONFIG}: configured issue #{number} is {}", latest.state).into());
        }
        if latest.updated_at != current.updated_at {
            continue;
        }
        if body != current.body {
            target.gh(
                root,
                &["issue", "edit", &number, "--body-file", "-"],
                Some(body.as_bytes()),
            )?;
        }
        return Ok(());
    }
    Err(format!("{CONFIG}: issue #{number} was busy after 3 attempts").into())
}

fn splice(current: &str, key: &str, section: &str, game: bool) -> Result<String> {
    let start = format!("<!-- aoe-workstream:{key}:start -->");
    let end = format!("<!-- aoe-workstream:{key}:end -->");
    let old = match (current.find(&start), current.find(&end)) {
        (Some(a), Some(b)) if a < b => Some((a, b + end.len())),
        (None, None) => None,
        _ => return Err(format!("{CONFIG}: malformed section {key}").into()),
    };
    let checked: HashSet<String> = old
        .map(|(a, b)| {
            current[a..b]
                .lines()
                .filter_map(|line| line.strip_prefix("- [x] ").map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let content = section
        .lines()
        .map(|line| {
            if let Some(rest) = line.strip_prefix("- [ ] ")
                && checked.contains(rest)
            {
                format!("- [x] {rest}")
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let replacement = if section.is_empty() {
        String::new()
    } else {
        format!("{start}\n{content}\n{end}")
    };
    if let Some((a, b)) = old {
        return Ok(format!("{}{}{}", &current[..a], replacement, &current[b..]));
    }
    if section.is_empty() {
        return Ok(current.to_owned());
    }
    if game {
        let mut after = None;
        let mut offset = 0;
        for line in current.split_inclusive('\n') {
            let heading = line.trim_end_matches(['\r', '\n']);
            let marks = heading.bytes().take_while(|byte| *byte == b'#').count();
            if (1..=6).contains(&marks)
                && heading[marks..]
                    .strip_prefix(" Game findings")
                    .is_some_and(|suffix| suffix.is_empty() || suffix.starts_with(' '))
            {
                after = Some(offset + heading.len());
                break;
            }
            offset += line.len();
        }
        let after = after.ok_or_else(|| {
            format!("{CONFIG}: game_review issue is missing its Game findings heading")
        })?;
        Ok(format!(
            "{}\n\n{}{}",
            &current[..after],
            replacement,
            &current[after..]
        ))
    } else {
        Ok(format!("{}\n\n{}\n", current, replacement))
    }
}

#[cfg(test)]
#[path = "tests/routing.rs"]
mod tests;
