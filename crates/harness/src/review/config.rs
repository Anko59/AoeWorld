//! `gates/review.json`: review tiers, model families per runtime, and the
//! minimum tier each registry suite demands. Parsed strictly.
use crate::agents::Runtime;
use serde::Deserialize;
use std::{collections::BTreeMap, collections::BTreeSet, path::Path};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Tier {
    Low,
    Medium,
    High,
    Xhigh,
    Max,
}

impl Tier {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Xhigh => "xhigh",
            Self::Max => "max",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Strength {
    Average,
    Strong,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TierConfig {
    pub(crate) personas: Vec<String>,
    pub(crate) max_rounds: u32,
    pub(crate) strength: Strength,
    pub(crate) reasoning_bump: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Model {
    pub(crate) model: String,
    pub(crate) effort: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Family {
    pub(crate) average: Model,
    pub(crate) strong: Model,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Config {
    pub(crate) version: u16,
    pub(crate) merge_grade: u8,
    pub(crate) tiers: BTreeMap<Tier, TierConfig>,
    pub(crate) models: BTreeMap<String, Family>,
    pub(crate) floors: BTreeMap<String, Tier>,
}

const PERSONAS: [&str; 6] = [
    "quick",
    "correctness",
    "spec",
    "test-integrity",
    "hostile-input",
    "performance",
];
/// The reasoning efforts each runtime accepts, lowest first.
pub(crate) fn efforts(runtime: &str) -> &'static [&'static str] {
    match runtime {
        "claude" => &["low", "medium", "high", "xhigh", "max"],
        "codex" => &["minimal", "low", "medium", "high", "xhigh"],
        "pi" => &["minimal", "low", "medium", "high", "xhigh"],
        _ => &["low", "medium", "high", "max"],
    }
}

impl Config {
    /// The trusted copy (origin/dev's): a branch cannot lower its own bar.
    pub(crate) fn load(root: &Path) -> Result<Self, String> {
        Self::parse(&super::trusted(root, "gates/review.json")?)
    }

    pub(crate) fn parse(text: &str) -> Result<Self, String> {
        let config: Self =
            serde_json::from_str(text).map_err(|e| format!("gates/review.json: {e}"))?;
        let mut problems = Vec::new();
        if config.version != 1 {
            problems.push("version must be 1".to_owned());
        }
        if !(1..=10).contains(&config.merge_grade) {
            problems.push("merge_grade must be 1-10".to_owned());
        }
        for tier in [Tier::Low, Tier::Medium, Tier::High, Tier::Xhigh, Tier::Max] {
            match config.tiers.get(&tier) {
                None => problems.push(format!("tier {} is missing", tier.name())),
                Some(t) => {
                    if t.personas.is_empty() || t.max_rounds == 0 {
                        problems.push(format!("tier {} needs personas and rounds", tier.name()));
                    }
                    let unique: BTreeSet<_> = t.personas.iter().collect();
                    if unique.len() != t.personas.len() {
                        problems.push(format!("tier {} repeats a persona", tier.name()));
                    }
                    for persona in &t.personas {
                        if !PERSONAS.contains(&persona.as_str()) {
                            problems.push(format!("unknown persona {persona}"));
                        }
                    }
                }
            }
        }
        for runtime in ["claude", "codex", "dsh", "pi"] {
            match config.models.get(runtime) {
                None => problems.push(format!("models for {runtime} are missing")),
                Some(family) => {
                    for model in [&family.average, &family.strong] {
                        if model.model.trim().is_empty()
                            || !efforts(runtime).contains(&model.effort.as_str())
                        {
                            problems.push(format!("{runtime}: invalid model or effort"));
                        }
                    }
                }
            }
        }
        if problems.is_empty() {
            Ok(config)
        } else {
            Err(format!("gates/review.json: {}", problems.join("; ")))
        }
    }

    /// The lowest tier the selected suites allow.
    pub(crate) fn floor(&self, suites: &BTreeSet<String>) -> Tier {
        suites
            .iter()
            .filter_map(|suite| self.floors.get(suite).copied())
            .max()
            .unwrap_or(Tier::Low)
    }

    /// The model for a review: a closing review (docs/review.md) runs on the
    /// strong models whatever the tier.
    pub(crate) fn model_for(&self, tier: Tier, runtime: Runtime, closing: bool) -> Model {
        let family = &self.models[runtime_key(runtime)];
        let tier_config = &self.tiers[&tier];
        let mut model = match (closing, tier_config.strength) {
            (true, _) | (_, Strength::Strong) => family.strong.clone(),
            (false, Strength::Average) => family.average.clone(),
        };
        if tier_config.reasoning_bump {
            model.effort = bump(runtime_key(runtime), &model.effort).to_owned();
        }
        model
    }
}

pub(crate) fn runtime_key(runtime: Runtime) -> &'static str {
    match runtime {
        Runtime::Claude => "claude",
        Runtime::Codex => "codex",
        Runtime::Dsh => "dsh",
        Runtime::Pi => "pi",
    }
}

/// One reasoning step higher, capped at the runtime's highest effort.
pub(crate) fn bump(runtime: &str, effort: &str) -> &'static str {
    let levels = efforts(runtime);
    let at = levels.iter().position(|e| *e == effort).unwrap_or(0);
    levels[(at + 1).min(levels.len() - 1)]
}
