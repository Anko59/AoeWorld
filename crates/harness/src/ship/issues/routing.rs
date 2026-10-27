//! Numbered workstream destinations and comment filing.
use super::{Result, Target};
use serde::Deserialize;
use std::path::Path;

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
struct Snapshot {
    state: String,
}

pub(super) fn warn(number: u64, error: &dyn std::fmt::Display) {
    eprintln!("warning: could not file follow-ups in #{number} ({error}); file them by hand");
}

pub(super) fn comment(root: &Path, number: u64, body: &str) {
    let result = (|| -> Result<()> {
        let target = Target::direct(root)?;
        let number_text = number.to_string();
        let state: Snapshot = serde_json::from_str(&target.gh(
            root,
            &["issue", "view", &number_text, "--json", "state"],
            None,
        )?)?;
        if state.state != "OPEN" {
            return Err(format!("{CONFIG}: configured issue #{number} is {}", state.state).into());
        }
        target.gh(
            root,
            &["issue", "comment", &number_text, "--body-file", "-"],
            Some(body.as_bytes()),
        )?;
        Ok(())
    })();
    match result {
        Ok(()) => println!("filed follow-ups in #{number}"),
        Err(error) => warn(number, &error),
    }
}

#[cfg(test)]
#[path = "tests/routing.rs"]
mod tests;
