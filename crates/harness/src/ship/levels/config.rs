//! `gates/levels.json`: what each pull request level allows and the floor
//! rules that decide the lowest level a change may use. Parsed strictly and
//! read from origin/dev (`review::trusted`), so a branch cannot loosen its own
//! limits.
use super::{super::describe::Level, Change, Floor};
use serde::Deserialize;
use std::{collections::BTreeMap, path::Path};

const LEVELS: [Level; 4] = [Level::Low, Level::Medium, Level::High, Level::Max];

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Spec {
    /// Longest video in seconds; `null`: no video at this level.
    pub(crate) video_max_seconds: Option<u32>,
    pub(crate) narration_required: bool,
    /// Non-empty lines of `## Why`, its one grounding element included.
    pub(crate) why_max_lines: usize,
    pub(crate) what_max_lines: usize,
    /// Name of the metrics row set; `metrics.rs` decides what it shows.
    pub(crate) metrics: String,
    /// Share of merged pull requests this level aims for.
    pub(crate) target_percent: u32,
}

/// A rule matches when the change is at least `min_changed_lines` long and,
/// if it names suites or path prefixes, touches one of them.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Rule {
    pub(crate) level: Level,
    #[serde(default)]
    pub(crate) suites: Vec<String>,
    #[serde(default)]
    pub(crate) path_prefixes: Vec<String>,
    #[serde(default)]
    pub(crate) min_changed_lines: u64,
    pub(crate) reason: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Levels {
    pub(crate) version: u16,
    /// How many merged pull requests `make level-stats` reads.
    pub(crate) stats_window: usize,
    /// Lines a binary file counts as in the changed-line total.
    pub(crate) binary_file_lines: u64,
    pub(crate) levels: BTreeMap<Level, Spec>,
    pub(crate) floor: Vec<Rule>,
}

impl Levels {
    /// The limits compiled into this binary, which is built from origin/dev:
    /// for the showcase recorder, which has no repository at hand. `make ship`
    /// reads the file through `load`.
    /// A test pins that the committed file parses.
    pub(crate) fn embedded() -> Option<&'static Self> {
        static EMBEDDED: std::sync::OnceLock<Option<Levels>> = std::sync::OnceLock::new();
        EMBEDDED
            .get_or_init(|| Levels::parse(include_str!("../../../../../gates/levels.json")).ok())
            .as_ref()
    }

    /// The trusted copy (origin/dev's; the working tree while dev lacks it).
    pub(crate) fn load(root: &Path) -> Result<Self, String> {
        Self::parse(&crate::review::trusted(root, "gates/levels.json")?)
    }

    pub(crate) fn parse(text: &str) -> Result<Self, String> {
        let config: Self =
            serde_json::from_str(text).map_err(|e| format!("gates/levels.json: {e}"))?;
        let mut problems = Vec::new();
        if config.version != 1 {
            problems.push("version must be 1".to_owned());
        }
        if config.stats_window == 0 {
            problems.push("stats_window must be positive".to_owned());
        }
        let mut total = 0;
        for level in LEVELS {
            let Some(spec) = config.levels.get(&level) else {
                problems.push(format!("level {} is missing", level.name()));
                continue;
            };
            total += spec.target_percent;
            if spec.why_max_lines == 0 || spec.what_max_lines == 0 {
                problems.push(format!("level {} needs positive line limits", level.name()));
            }
            if spec.video_max_seconds == Some(0) {
                problems.push(format!(
                    "level {}: video_max_seconds must be positive or null",
                    level.name()
                ));
            }
            if spec.metrics.trim().is_empty() {
                problems.push(format!("level {} needs a metrics row set", level.name()));
            }
        }
        if total != 100 {
            problems.push(format!("target_percent must add up to 100, not {total}"));
        }
        for rule in &config.floor {
            if rule.reason.trim().is_empty() {
                problems.push("every floor rule needs a reason".to_owned());
            }
        }
        if problems.is_empty() {
            Ok(config)
        } else {
            Err(format!("gates/levels.json: {}", problems.join("; ")))
        }
    }

    /// The suites a floor rule names must exist: a misspelt one never matches.
    pub(crate) fn check_suites<'a>(
        &self,
        known: impl Iterator<Item = &'a str>,
    ) -> Result<(), String> {
        let known: Vec<&str> = known.collect();
        for rule in &self.floor {
            if let Some(unknown) = rule.suites.iter().find(|s| !known.contains(&s.as_str())) {
                return Err(format!(
                    "gates/levels.json: floor rule for unknown suite `{unknown}`"
                ));
            }
        }
        Ok(())
    }

    pub(crate) fn spec(&self, level: Level) -> &Spec {
        &self.levels[&level]
    }

    /// The lowest level with a video, for the message to a `low` PR that has one.
    pub(crate) fn lowest_with_video(&self) -> Level {
        LEVELS
            .into_iter()
            .find(|level| self.spec(*level).video_max_seconds.is_some())
            .unwrap_or(Level::Max)
    }

    /// The highest level any matching rule demands, and why.
    pub(crate) fn floor(&self, change: &Change) -> Floor {
        let mut floor = Floor {
            level: Level::Low,
            reason: "docs, configuration or a small change".to_owned(),
        };
        for rule in &self.floor {
            let scoped = !rule.suites.is_empty() || !rule.path_prefixes.is_empty();
            let touches = rule.suites.iter().any(|s| change.suites.contains(s))
                || rule
                    .path_prefixes
                    .iter()
                    .any(|p| change.files.iter().any(|f| f.starts_with(p.as_str())));
            if change.lines >= rule.min_changed_lines
                && (!scoped || touches)
                && rule.level > floor.level
            {
                floor = Floor {
                    level: rule.level,
                    reason: rule.reason.clone(),
                };
            }
        }
        floor
    }
}
