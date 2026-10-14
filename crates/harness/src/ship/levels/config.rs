//! `gates/levels.json`: what each pull request level allows and the floor
//! rules that decide the lowest level a change may use. Parsed strictly and
//! read from origin/dev (`review::trusted`), so a branch cannot loosen its own
//! limits.
use super::{super::describe::Level, Change, Floor};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};

const LEVELS: [Level; 4] = [Level::Low, Level::Medium, Level::High, Level::Max];

/// The metrics row set of a level: each set shows the rows up to its tier
/// (`ship/metrics/table.rs` tags every row with the lowest tier showing it).
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum RowSet {
    Summary,
    Standard,
    Extended,
    Full,
}

impl RowSet {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Summary => "summary",
            Self::Standard => "standard",
            Self::Extended => "extended",
            Self::Full => "full",
        }
    }

    /// The highest row tier the set shows.
    pub(crate) fn tier(self) -> Level {
        match self {
            Self::Summary => Level::Low,
            Self::Standard => Level::Medium,
            Self::Extended => Level::High,
            Self::Full => Level::Max,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Spec {
    /// Longest video in seconds; `null`: no video at this level.
    pub(crate) video_max_seconds: Option<u32>,
    pub(crate) narration_required: bool,
    /// Non-empty lines of `## Why`, its one grounding element included.
    pub(crate) why_max_lines: usize,
    pub(crate) what_max_lines: usize,
    /// The metrics rows the pull request description shows.
    pub(crate) metrics: RowSet,
    /// Share of merged pull requests this level aims for.
    pub(crate) target_percent: u32,
}

/// A rule matches when the lines it covers are at least `min_changed_lines` and,
/// if it names suites or path prefixes, a file of the change is in one of them.
/// A scoped rule covers only the changed lines of its own files; an unscoped
/// one covers the whole change.
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
    /// Only files that can change behaviour count towards `suites`: test files
    /// and the `inert` paths do not.
    #[serde(default)]
    pub(crate) behaviour: bool,
    /// What the covered lines are, for "N of M changed lines" in the reason of
    /// a scoped rule with a minimum (required then).
    #[serde(default)]
    pub(crate) subject: Option<String>,
    pub(crate) reason: String,
}

impl Rule {
    fn scoped(&self) -> bool {
        !self.suites.is_empty() || !self.path_prefixes.is_empty()
    }

    /// The changed lines this rule covers and whether it touches any file.
    fn covered(&self, change: &Change) -> (u64, bool) {
        if !self.scoped() {
            return (change.lines, true);
        }
        let mut lines = 0;
        let mut touched = false;
        for part in &change.parts {
            if self.behaviour && !part.behaviour {
                continue;
            }
            let hit = self.suites.iter().any(|s| part.suites.contains(s))
                || self
                    .path_prefixes
                    .iter()
                    .any(|p| part.path.starts_with(p.as_str()));
            if hit {
                lines += part.lines;
                touched = true;
            }
        }
        (lines, touched)
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Levels {
    pub(crate) version: u16,
    /// How many merged pull requests `make level-stats` reads.
    pub(crate) stats_window: usize,
    /// Lines a binary file counts as in the changed-line total.
    pub(crate) binary_file_lines: u64,
    /// Documentation and configuration paths: a path that starts with a
    /// prefix or ends with a suffix changes no product behaviour by itself.
    pub(crate) inert_prefixes: Vec<String>,
    pub(crate) inert_suffixes: Vec<String>,
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
        }
        if total != 100 {
            problems.push(format!("target_percent must add up to 100, not {total}"));
        }
        for rule in &config.floor {
            if rule.reason.trim().is_empty() {
                problems.push("every floor rule needs a reason".to_owned());
            }
            let counted = rule.min_changed_lines > 0 && rule.scoped();
            if counted && rule.subject.as_deref().is_none_or(|s| s.trim().is_empty()) {
                problems.push(format!(
                    "floor rule `{}` counts its own lines and needs a subject",
                    rule.reason
                ));
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

    /// Whether a file is documentation or configuration (see `inert_prefixes`).
    pub(crate) fn is_inert(&self, path: &str) -> bool {
        self.inert_prefixes
            .iter()
            .any(|p| path.starts_with(p.as_str()))
            || self
                .inert_suffixes
                .iter()
                .any(|s| path.ends_with(s.as_str()))
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

    /// The highest level any matching rule demands, and why. A scoped rule
    /// with a minimum counts only the changed lines in its own paths, and when
    /// those are not all of the change the reason says how many they are.
    pub(crate) fn floor(&self, change: &Change) -> Floor {
        let mut top: Option<(usize, &Rule)> = None;
        let mut reasons = Vec::new();
        let mut detailed = Vec::new();
        for (index, rule) in self.floor.iter().enumerate() {
            let (lines, touched) = rule.covered(change);
            if rule.min_changed_lines > 0 && rule.scoped() && touched && lines != change.lines {
                let subject = rule.subject.as_deref().unwrap_or_default();
                reasons.push(format!(
                    "{subject}: {lines} of {} changed lines (need {} for {})",
                    change.lines,
                    rule.min_changed_lines,
                    rule.level.name()
                ));
                detailed.push(index);
            }
            let applies = lines >= rule.min_changed_lines && (!rule.scoped() || touched);
            if applies && top.is_none_or(|(_, best)| rule.level > best.level) {
                top = Some((index, rule));
            }
        }
        let level = match top {
            Some((index, rule)) => {
                if !detailed.contains(&index) {
                    reasons.push(rule.reason.clone());
                }
                rule.level
            }
            None => {
                reasons.push("docs, configuration or a small change".to_owned());
                Level::Low
            }
        };
        Floor {
            level,
            reason: reasons.join("; "),
        }
    }
}
