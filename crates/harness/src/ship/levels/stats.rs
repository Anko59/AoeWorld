//! The distribution of levels over the last merged pull requests on dev,
//! read from the level each published description names.
use super::{super::describe::Level, Levels};
use serde::Deserialize;
use std::{collections::BTreeMap, path::Path};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// The level a description names: the `<!-- level: … -->` line of a body
/// file, or the published form `## Gate evidence (`<sha>`, level <name>)`.
pub(crate) fn marker(body: &str) -> Option<Level> {
    body.lines().find_map(|line| {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("<!-- level:") {
            return rest.strip_suffix("-->").and_then(Level::parse);
        }
        let (_, rest) = line
            .strip_prefix("## Gate evidence")?
            .rsplit_once(", level ")?;
        Level::parse(rest.strip_suffix(')')?)
    })
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Distribution {
    pub(crate) counts: BTreeMap<Level, u32>,
    /// Pull requests without a marker: older descriptions.
    pub(crate) unmarked: u32,
}

impl Distribution {
    pub(crate) fn from_bodies<'a>(bodies: impl IntoIterator<Item = &'a str>) -> Self {
        let mut found = Self::default();
        for body in bodies {
            match marker(body) {
                Some(level) => *found.counts.entry(level).or_default() += 1,
                None => found.unmarked += 1,
            }
        }
        found
    }

    fn marked(&self) -> u32 {
        self.counts.values().sum()
    }

    fn percent(&self, level: Level) -> u32 {
        let marked = self.marked();
        if marked == 0 {
            return 0;
        }
        let count = self.counts.get(&level).copied().unwrap_or(0);
        (f64::from(count) * 100.0 / f64::from(marked)).round() as u32
    }

    /// One line for the "For AI" part.
    pub(crate) fn line(&self, levels: &Levels) -> String {
        let parts: Vec<String> = levels
            .levels
            .iter()
            .map(|(level, spec)| {
                format!(
                    "{} {}% (target {}%)",
                    level.name(),
                    self.percent(*level),
                    spec.target_percent
                )
            })
            .collect();
        format!(
            "Level distribution of the last {} merged PRs ({} with a level marker): {}",
            self.marked() + self.unmarked,
            self.marked(),
            parts.join(", ")
        )
    }

    pub(crate) fn table(&self, levels: &Levels) -> String {
        let mut out = format!(
            "{} merged PRs read, {} with a level marker\n\n| Level | PRs | Actual | Target |\n|---|---|---|---|\n",
            self.marked() + self.unmarked,
            self.marked()
        );
        for (level, spec) in &levels.levels {
            out.push_str(&format!(
                "| {} | {} | {}% | {}% |\n",
                level.name(),
                self.counts.get(level).copied().unwrap_or(0),
                self.percent(*level),
                spec.target_percent
            ));
        }
        out.push_str(&format!("| (no marker) | {} | | |", self.unmarked));
        out
    }
}

#[derive(Deserialize)]
struct Merged {
    body: Option<String>,
}

/// The distribution over the last `stats_window` pull requests merged into dev.
pub(crate) fn read(root: &Path, levels: &Levels) -> Result<Distribution> {
    let repository = super::super::git::origin_repository(root)?;
    let limit = levels.stats_window.to_string();
    let json = super::super::gh(
        root,
        &[
            "pr",
            "list",
            "--repo",
            &repository,
            "--state",
            "merged",
            "--base",
            "dev",
            "--limit",
            &limit,
            "--json",
            "body",
        ],
    )?;
    let merged: Vec<Merged> = serde_json::from_str(&json)?;
    Ok(Distribution::from_bodies(
        merged
            .iter()
            .map(|pr| pr.body.as_deref().unwrap_or_default()),
    ))
}

/// The "For AI" line; best effort, a `gh` failure only says it is unavailable.
pub(crate) fn summary_line(root: &Path, levels: &Levels) -> String {
    match read(root, levels) {
        Ok(found) => found.line(levels),
        Err(error) => format!("Level distribution unavailable: {error}"),
    }
}
