//! `gates/ci-rules.json`: what a pull request's changed paths mean for the CI jobs.
//!
//! Crates are not listed here. A path inside a workspace crate selects the jobs through
//! the crate dependency graph; this file names only the jobs' subject crates, the
//! paths that are not crate sources, and the harness modules that may narrow the
//! "a change to the harness runs everything" default.
use crate::gates::registry::{Registry, glob::glob, helpers::valid_pattern};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, error::Error, fs, path::Path};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

/// The crate that owns the harness sources and the files its tests read.
pub(crate) const HARNESS: &str = "aoe-harness";
const HARNESS_SOURCES: &str = "crates/harness/";

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct JobRule {
    /// The crates the job builds or tests: it runs when a changed crate is in their closure.
    pub(crate) crates: Vec<String>,
    /// Whether the job runs the crates' tests, so their dev-dependencies count too.
    pub(crate) tests: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StaticParts {
    pub(crate) light: Vec<String>,
    pub(crate) checks: Vec<String>,
    pub(crate) unit: Vec<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Effect {
    /// An input of the harness tests (instructions, skills): they run, nothing else.
    Harness,
    /// The marker where a crate outside the workspace owns the path.
    OutsideWorkspace,
    /// Every job, the whole workspace's tests, the full static checks.
    Everything,
    /// The static checks that compile (clippy, deny, the wasm build), no tests.
    Checks,
    /// The static checks that compile nothing, and nothing else.
    Static,
    /// A file some crate's tests may read: the crates whose sources name it run their tests.
    Reads,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PathRule {
    pub(crate) effect: Effect,
    pub(crate) globs: Vec<String>,
    #[serde(default)]
    pub(crate) jobs: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ModuleRule {
    pub(crate) globs: Vec<String>,
    /// The jobs whose plumbing lives in these modules.
    pub(crate) jobs: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Rules {
    pub(crate) version: u16,
    /// Directories of crates that are not workspace members.
    pub(crate) outside_workspace: Vec<String>,
    pub(crate) jobs: BTreeMap<String, JobRule>,
    pub(crate) static_parts: StaticParts,
    /// Crates whose change selects the `perf-smoke` gate of the unit leg.
    pub(crate) perf_smoke_crates: Vec<String>,
    /// First match wins.
    pub(crate) paths: Vec<PathRule>,
    pub(crate) harness_modules: Vec<ModuleRule>,
}

/// What a changed path is, before the crate graph is consulted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Class {
    /// Run everything; the string names the rule.
    Everything(String),
    Checks,
    Static,
    Harness,
    Reads(Vec<String>),
    /// Owned by a crate; `jobs` are selected on top of the graph.
    Crate {
        name: String,
        jobs: Vec<String>,
    },
    /// No rule covers it: treated as `Everything`, and a test forbids it for tracked paths.
    Unknown,
}

fn any(globs: &[String], path: &str) -> bool {
    globs.iter().any(|g| glob(g, path))
}

impl Rules {
    pub(crate) fn load(root: &Path, registry: &Registry) -> Result<Self> {
        Self::parse(&fs::read(root.join("gates/ci-rules.json"))?, Some(registry))
    }

    /// A hash of the rules, bound into the manifest next to the registry's.
    pub(crate) fn fingerprint(&self) -> Result<String> {
        let mut hash = blake3::Hasher::new();
        hash.update(b"aoe-ci-rules-v1\0");
        hash.update(&serde_json::to_vec(self)?);
        Ok(format!("blake3:ci-rules-v1:{}", hash.finalize().to_hex()))
    }

    /// Parse and validate; with a registry also check the job and gate names.
    pub(crate) fn parse(bytes: &[u8], registry: Option<&Registry>) -> Result<Self> {
        let rules: Self = serde_json::from_slice(bytes)?;
        let mut problems = Vec::new();
        if rules.version != 1 {
            problems.push("ci rules version must be 1".to_owned());
        }
        let markers = rules
            .paths
            .iter()
            .filter(|r| r.effect == Effect::OutsideWorkspace)
            .count();
        if markers != 1 {
            problems.push(format!(
                "exactly one outside-workspace rule is required, found {markers}"
            ));
        }
        let globs = rules
            .paths
            .iter()
            .flat_map(|r| &r.globs)
            .chain(rules.harness_modules.iter().flat_map(|m| &m.globs));
        for pattern in globs {
            if !valid_pattern(pattern) {
                problems.push(format!("invalid glob {pattern}"));
            }
        }
        let named_jobs = rules
            .paths
            .iter()
            .flat_map(|r| &r.jobs)
            .chain(rules.harness_modules.iter().flat_map(|m| &m.jobs));
        for job in named_jobs {
            if !rules.jobs.contains_key(job) {
                problems.push(format!("path rule names job {job} that has no crate rule"));
            }
        }
        if let Some(registry) = registry {
            rules.check_registry(registry, &mut problems);
        }
        if problems.is_empty() {
            Ok(rules)
        } else {
            Err(problems.join("\n").into())
        }
    }

    fn check_registry(&self, registry: &Registry, problems: &mut Vec<String>) {
        for job in self.jobs.keys() {
            if !registry.jobs.contains_key(job) {
                problems.push(format!("ci rules name unknown job {job}"));
            }
        }
        let parts = &self.static_parts;
        let mut mapped: Vec<_> = [&parts.light, &parts.checks, &parts.unit]
            .into_iter()
            .flatten()
            .cloned()
            .collect();
        mapped.sort();
        let mut members = registry.jobs.get("static").cloned().unwrap_or_default();
        members.sort();
        if mapped != members {
            problems.push(format!(
                "static_parts must map each member of the static job exactly once: \
                 mapped {mapped:?}, job {members:?}"
            ));
        }
    }

    /// The part of the static job that runs `gate`.
    pub(crate) fn part_of(&self, gate: &str) -> Option<&'static str> {
        let parts = &self.static_parts;
        [
            ("light", &parts.light),
            ("checks", &parts.checks),
            ("unit", &parts.unit),
        ]
        .into_iter()
        .find(|(_, gates)| gates.iter().any(|g| g == gate))
        .map(|(part, _)| part)
    }

    /// Whether `path` is an input a crate's tests may read, which needs the crates' sources.
    pub(crate) fn reads(&self, path: &str) -> bool {
        self.paths
            .iter()
            .find(|r| any(&r.globs, path))
            .is_some_and(|r| r.effect == Effect::Reads)
    }

    fn outside_owner<'a>(&'a self, path: &str) -> Option<&'a str> {
        self.outside_workspace
            .iter()
            .map(String::as_str)
            .find(|dir| path.strip_prefix(dir).is_some_and(|r| r.starts_with('/')))
    }

    /// Classify one path. `owner` gives the workspace crate that owns it, if any.
    pub(crate) fn classify(&self, path: &str, owner: impl Fn(&str) -> Option<String>) -> Class {
        for rule in &self.paths {
            if rule.effect == Effect::OutsideWorkspace {
                if let Some(name) = self.outside_owner(path).and_then(|_| owner(path)) {
                    return Class::Crate {
                        name,
                        jobs: Vec::new(),
                    };
                }
                continue;
            }
            if !any(&rule.globs, path) {
                continue;
            }
            let label = rule.globs.first().map_or("rule", String::as_str);
            return match rule.effect {
                Effect::Harness => Class::Harness,
                Effect::Everything => Class::Everything(format!("protected path {label}")),
                Effect::Checks => Class::Checks,
                Effect::Static => Class::Static,
                Effect::Reads => Class::Reads(rule.jobs.clone()),
                Effect::OutsideWorkspace => unreachable!("handled above"),
            };
        }
        let Some(name) = owner(path) else {
            return Class::Unknown;
        };
        if path.starts_with(HARNESS_SOURCES) {
            return match self.harness_modules.iter().find(|m| any(&m.globs, path)) {
                Some(module) => Class::Crate { name, jobs: module.jobs.clone() },
                None => Class::Everything(
                    "a harness module no ci rule narrows (the harness is the plumbing of every job)"
                        .to_owned(),
                ),
            };
        }
        Class::Crate {
            name,
            jobs: Vec::new(),
        }
    }
}
