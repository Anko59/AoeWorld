//! Executable gate policy: strict selection and dependency-first cadence plans.
mod fingerprint;
mod glob;
mod helpers;
use glob::glob;
use helpers::*;

use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fs,
    path::Path,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Debug, Clone, Copy, Deserialize, Serialize, Eq, Ord, PartialEq, PartialOrd)]
#[serde(rename_all = "kebab-case")]
#[derive(clap::ValueEnum)]
pub(crate) enum Cadence {
    Edit,
    Stop,
    Commit,
    Pr,
    Ci,
    Nightly,
    Weekly,
    Qualification,
    Preflight,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, Eq, Ord, PartialEq, PartialOrd)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Capability {
    Docker,
    SourceAssets,
    SourceGeodata,
    Hardware,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Suite {
    pub(crate) id: String,
    pub(crate) paths: Vec<String>,
    pub(crate) implies: Vec<String>,
    pub(crate) review: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Gate {
    pub(crate) id: String,
    pub(crate) command: String,
    pub(crate) requires: Vec<String>,
    /// Retained v1 explanatory label; selection uses suites and cadences only.
    pub(crate) select: String,
    pub(crate) evidence: String,
    pub(crate) suites: Vec<String>,
    pub(crate) cadences: Vec<Cadence>,
    /// Scheduling estimate, not a workload threshold or a permission to skip.
    pub(crate) budget_s: u32,
    /// Describes the check; does NOT make it unconditional.
    pub(crate) r#static: bool,
    pub(crate) capabilities: Vec<Capability>,
    /// Failure/incompleteness blocks these cadences; runner must enforce later.
    pub(crate) blocks: Vec<Cadence>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Registry {
    pub(crate) version: u16,
    pub(crate) suites: Vec<Suite>,
    pub(crate) gates: Vec<Gate>,
    /// Validation jobs only. Release workflow orchestration remains privileged.
    #[serde(deserialize_with = "unique_jobs")]
    pub(crate) jobs: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Classification {
    pub(crate) suites: BTreeSet<String>,
    pub(crate) reasons: BTreeMap<String, BTreeSet<String>>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Plan {
    pub(crate) cadence: Cadence,
    pub(crate) suites: BTreeSet<String>,
    /// Dependency-first gate IDs; lexicographic tie breaking.
    pub(crate) gates: Vec<String>,
    pub(crate) jobs: BTreeMap<String, bool>,
}

impl Registry {
    pub(crate) fn fingerprint(&self) -> Result<String> {
        fingerprint::fingerprint(self)
    }

    pub(crate) fn load(root: &Path) -> Result<Self> {
        Self::parse(&fs::read(root.join("gates/registry.json"))?)
    }

    pub(crate) fn parse(bytes: &[u8]) -> Result<Self> {
        // Structural/unknown-field errors are serde errors. All semantic errors
        // in a structurally valid document are collected in one diagnostic.
        let registry: Self = serde_json::from_slice(bytes)?;
        registry.validate()?;
        Ok(registry)
    }

    fn validate(&self) -> Result<()> {
        let mut problems = Vec::new();
        if self.version != 2 {
            problems.push("registry version must be 2".into());
        }
        if self.gates.is_empty() {
            problems.push("empty gate catalog".into());
        }
        let mut suites = BTreeSet::new();
        let mut gates = BTreeSet::new();
        for suite in &self.suites {
            if !identifier(&suite.id) || !suites.insert(suite.id.clone()) {
                problems.push(format!("invalid or duplicate suite {}", suite.id));
            }
            duplicates(&suite.paths, &format!("{} paths", suite.id), &mut problems);
            duplicates(
                &suite.implies,
                &format!("{} implies", suite.id),
                &mut problems,
            );
            for pattern in &suite.paths {
                if !valid_pattern(pattern) {
                    problems.push(format!("invalid glob {pattern} for {}", suite.id));
                }
            }
        }
        for mandatory in ["everything", "static"] {
            if !suites.contains(mandatory) {
                problems.push(format!("missing mandatory suite {mandatory}"));
            }
        }
        for suite in &self.suites {
            references(&suite.implies, &suites, &suite.id, "suite", &mut problems);
            if suite.implies.contains(&suite.id) {
                problems.push(format!("self implication {}", suite.id));
            }
        }
        let suite_graph: BTreeMap<_, _> = self
            .suites
            .iter()
            .map(|s| (s.id.clone(), s.implies.clone()))
            .collect();
        unresolved(&suite_graph, "suite", &mut problems);
        if suites.contains("everything")
            && self.expand(&BTreeSet::from(["everything".into()])) != suites
        {
            problems.push("everything must imply all declared suites".into());
        }
        for gate in &self.gates {
            if !identifier(&gate.id) || !gates.insert(gate.id.clone()) {
                problems.push(format!("invalid or duplicate gate {}", gate.id));
            }
            if gate.command != format!("make {}", gate.id) {
                problems.push(format!("command mismatch for {}", gate.id));
            }
            if gate.select.trim().is_empty()
                || gate.evidence.trim().is_empty()
                || gate.suites.is_empty()
                || gate.budget_s == 0
            {
                problems.push(format!("empty metadata or zero budget for {}", gate.id));
            }
            duplicates(&gate.requires, &gate.id, &mut problems);
            duplicates(&gate.suites, &gate.id, &mut problems);
            duplicates(&gate.cadences, &gate.id, &mut problems);
            duplicates(&gate.capabilities, &gate.id, &mut problems);
            duplicates(&gate.blocks, &gate.id, &mut problems);
            references(&gate.suites, &suites, &gate.id, "suite", &mut problems);
            if gate.blocks.iter().any(|c| !gate.cadences.contains(c)) {
                problems.push(format!("blocks outside declared cadences for {}", gate.id));
            }
            // Publishing is never available via the generic cadence API.
            if gate.id == "release-publish" && !gate.cadences.is_empty() {
                problems.push("release-publish requires explicit privileged dispatch".into());
            }
        }
        for gate in &self.gates {
            references(&gate.requires, &gates, &gate.id, "gate", &mut problems);
            if gate.requires.contains(&gate.id) {
                problems.push(format!("self dependency {}", gate.id));
            }
        }
        let graph: BTreeMap<_, _> = self
            .gates
            .iter()
            .map(|g| (g.id.clone(), g.requires.clone()))
            .collect();
        unresolved(&graph, "gate", &mut problems);
        for (job, members) in &self.jobs {
            if !identifier(job) || members.is_empty() {
                problems.push(format!("invalid or empty job {job}"));
            }
            duplicates(members, job, &mut problems);
            references(members, &gates, job, "job gate", &mut problems);
        }
        let job_roots: BTreeSet<_> = self.jobs.values().flatten().cloned().collect();
        for gate in &self.gates {
            if gate.cadences.contains(&Cadence::Ci) && !job_roots.contains(&gate.id) {
                problems.push(format!("CI gate {} has no job coverage", gate.id));
            }
            if job_roots.contains(&gate.id) && !gate.cadences.contains(&Cadence::Ci) {
                problems.push(format!("job root {} is not a CI gate", gate.id));
            }
        }
        // A declared dependency may add work but never introduce publication.
        for gate in &self.gates {
            if !gate.cadences.is_empty() {
                let closure = dependency_closure(&BTreeSet::from([gate.id.clone()]), &graph);
                if closure.contains("release-publish") {
                    problems.push(format!("{} reaches privileged release-publish", gate.id));
                }
            }
        }
        if problems.is_empty() {
            Ok(())
        } else {
            Err(problems.join("\n").into())
        }
    }

    fn expand(&self, initial: &BTreeSet<String>) -> BTreeSet<String> {
        let graph = self
            .suites
            .iter()
            .map(|s| (s.id.clone(), s.implies.clone()))
            .collect();
        dependency_closure(initial, &graph)
    }

    /// Caller supplies both rename names (PR1 paths::changed uses --no-renames).
    /// Empty input means unavailable/no comparison: fail conservatively to all.
    pub(crate) fn classify(&self, paths: &[String]) -> Classification {
        let mut result = Classification {
            suites: BTreeSet::from(["static".into()]),
            reasons: BTreeMap::new(),
        };
        result
            .reasons
            .entry("static".into())
            .or_default()
            .insert("baseline".into());
        if paths.is_empty() {
            result
                .reasons
                .entry("everything".into())
                .or_default()
                .insert("no comparison paths".into());
            result.suites.insert("everything".into());
        }
        for path in paths {
            let protected = instruction_path(path)
                || self
                    .suites
                    .iter()
                    .filter(|s| s.id == "everything")
                    .any(|s| s.paths.iter().any(|p| glob(p, path)));
            let matched: Vec<_> = self
                .suites
                .iter()
                .filter(|s| s.id != "everything")
                .filter(|s| s.paths.iter().any(|p| glob(p, path)))
                .collect();
            if protected || !valid_path(path) || matched.is_empty() {
                result.suites.insert("everything".into());
                result
                    .reasons
                    .entry("everything".into())
                    .or_default()
                    .insert(format!(
                        "{}: {path}",
                        if protected { "protected" } else { "unknown" }
                    ));
            } else {
                for suite in matched {
                    result.suites.insert(suite.id.clone());
                    result
                        .reasons
                        .entry(suite.id.clone())
                        .or_default()
                        .insert(format!("path: {path}"));
                }
            }
        }
        // At most one round per declared owner plus a final pass covers the
        // finite graph and every implication reason, including shared edges.
        // Bound traversal rather than relying on convergence alone. Unknown
        // implied IDs have no outgoing edges.
        for _ in 0..=self.suites.len() {
            let before = result.suites.len();
            for suite in &self.suites {
                if result.suites.contains(&suite.id) {
                    for implied in &suite.implies {
                        result.suites.insert(implied.clone());
                        result
                            .reasons
                            .entry(implied.clone())
                            .or_default()
                            .insert(format!("implied by {}", suite.id));
                    }
                }
            }
            if before == result.suites.len() {
                break;
            }
        }
        result
    }

    pub(crate) fn plan(&self, cadence: Cadence, suites: &BTreeSet<String>) -> Result<Plan> {
        self.validate()?;
        let known = self.suites.iter().map(|s| s.id.clone()).collect();
        let mut problems = Vec::new();
        references(
            &suites.iter().cloned().collect::<Vec<_>>(),
            &known,
            "plan",
            "suite",
            &mut problems,
        );
        if !problems.is_empty() {
            return Err(problems.join("\n").into());
        }
        let mut initial = suites.clone();
        initial.insert("static".into());
        let suites = self.expand(&initial);
        let graph: BTreeMap<_, _> = self
            .gates
            .iter()
            .map(|g| (g.id.clone(), g.requires.clone()))
            .collect();
        let seeds = self
            .gates
            .iter()
            .filter(|g| g.cadences.contains(&cadence))
            .filter(|g| {
                cadence == Cadence::Preflight || g.suites.iter().any(|s| suites.contains(s))
            })
            .map(|g| g.id.clone())
            .collect();
        let selected = dependency_closure(&seeds, &graph);
        let mut remaining = selected.clone();
        let mut done = BTreeSet::new();
        let mut gates = Vec::new();
        while !remaining.is_empty() {
            let next = remaining
                .iter()
                .find(|id| graph[*id].iter().all(|d| done.contains(d)))
                .cloned()
                .ok_or("unresolved gate plan")?;
            remaining.remove(&next);
            done.insert(next.clone());
            gates.push(next);
        }
        let jobs = self
            .jobs
            .iter()
            .map(|(job, members)| (job.clone(), members.iter().any(|id| selected.contains(id))))
            .collect();
        Ok(Plan {
            cadence,
            suites,
            gates,
            jobs,
        })
    }
}

fn unique_jobs<'de, D>(
    deserializer: D,
) -> std::result::Result<BTreeMap<String, Vec<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct Jobs;
    impl<'de> serde::de::Visitor<'de> for Jobs {
        type Value = BTreeMap<String, Vec<String>>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a job map with unique names")
        }
        fn visit_map<M>(self, mut map: M) -> std::result::Result<Self::Value, M::Error>
        where
            M: serde::de::MapAccess<'de>,
        {
            let mut jobs = BTreeMap::new();
            while let Some((key, value)) = map.next_entry::<String, Vec<String>>()? {
                if jobs.insert(key.clone(), value).is_some() {
                    return Err(serde::de::Error::custom(format!("duplicate job {key}")));
                }
            }
            Ok(jobs)
        }
    }
    deserializer.deserialize_map(Jobs)
}

#[cfg(test)]
mod tests;
