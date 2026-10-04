//! Shared execution. Candidate Make execution is feedback, never a trusted judge.
mod cli;
pub(crate) mod evidence;
mod real;
pub(crate) mod signals;
pub(crate) use cli::{Options, execute};
#[cfg(test)]
mod tests;

use crate::gates::registry::{Cadence, Capability, Gate, Plan, Registry};
use evidence::{EndpointProof, Ledger, Metadata};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

/// Canonical, explicit repository root. No process-global cwd changes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ExecutionRoot(PathBuf);
impl ExecutionRoot {
    pub(crate) fn new(path: &Path) -> Result<Self> {
        let root = fs::canonicalize(path)?;
        if !root.join("Makefile").is_file() {
            return Err("missing snapshot Makefile".into());
        }
        Ok(Self(root))
    }
    pub(crate) fn path(&self) -> &Path {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct GateId(String);
impl GateId {
    fn new(id: &str) -> Result<Self> {
        let valid = !id.is_empty()
            && id.as_bytes()[0].is_ascii_lowercase()
            && id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
        // Defense in depth; protected catalog/Make trust remains necessary.
        let privileged = id
            .split('-')
            .any(|s| matches!(s, "publish" | "approve" | "approval" | "merge"));
        if !valid || privileged {
            return Err(format!("invalid or privileged gate {id:?}").into());
        }
        Ok(Self(id.to_owned()))
    }
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum Verdict {
    Pass,
    Fail,
    Unavailable,
    Skipped,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum Overall {
    Pass,
    Fail,
    Incomplete,
    Invalid,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct GateResult {
    pub(crate) gate: GateId,
    pub(crate) verdict: Verdict,
    pub(crate) duration_ms: u64,
    pub(crate) reason: String,
    pub(crate) blocked_by: Vec<String>,
    pub(crate) unavailable: Vec<Capability>,
    pub(crate) log: Option<LogRef>,
    pub(crate) blocks_cadence: bool,
}
#[derive(Clone, Debug, Serialize)]
pub(crate) struct LogRef {
    /// Launcher-owned external file, with stdout AND stderr, even on success.
    pub(crate) path: PathBuf,
    pub(crate) blake3: String,
    pub(crate) truncated: bool,
}
#[derive(Clone, Debug, Serialize)]
pub(crate) enum CapabilityState {
    Available { observation: String },
    Unavailable { reason: String },
}
pub(crate) type Capabilities = BTreeMap<Capability, CapabilityState>;
#[derive(Clone, Copy, Debug, Serialize)]
pub(crate) struct Budgets {
    pub(crate) total: Duration,
    pub(crate) per_gate_max: Duration,
}
#[derive(Clone, Copy, Debug)]
pub(crate) enum Exit {
    Success,
    Failed,
    Deadline,
    Cancelled,
    StartError,
    MonitorError,
}
pub(crate) struct Receipt {
    pub(crate) exit: Exit,
    pub(crate) reason: String,
    pub(crate) log: Option<LogRef>,
}

/// No arbitrary program/string-shell API. Real adapter calls ONLY supervised
/// make with exactly one validated ID and Command.current_dir(root.path()).
pub(crate) trait Runtime {
    fn now_ms(&self) -> u64;
    fn cancelled(&self) -> bool;
    fn capabilities(&mut self) -> Capabilities;
    /// Scope-aware ORIGINAL source bytes + identity: Working raw tracked/nonignored
    /// untracked bytes; Index exact index/HEAD/index bytes and referenced object bytes;
    /// Commit fixed commit/tree/blob bytes plus expected source HEAD policy.
    /// AND private raw blob bytes/modes/HEAD/index against preparation identity.
    /// Must also validate the initial baseline, not merely compare two new probes.
    fn verify(&mut self, root: &ExecutionRoot) -> Result<EndpointProof>;
    fn make(&mut self, root: &ExecutionRoot, gate: &GateId, deadline: Duration) -> Receipt;
}

pub(crate) enum Selection<'a> {
    Cadence {
        cadence: Cadence,
        suites: &'a BTreeSet<String>,
    },
    CiJob(&'a str),
}
pub(crate) struct PreparedPlan {
    plan: Plan,
    gates: Vec<(GateId, Gate)>,
    registry_hash: String,
}
impl PreparedPlan {
    pub(crate) fn new(registry: &Registry, selection: Selection<'_>) -> Result<Self> {
        // Fields are currently mutable/pub(crate): reparse to enforce v2 validation.
        let registry = Registry::parse(&serde_json::to_vec(registry)?)?;
        let plan = match selection {
            Selection::Cadence { cadence, suites } => registry.plan(cadence, suites)?,
            Selection::CiJob(job) => {
                let members = registry.jobs.get(job).ok_or("unknown CI job")?;
                let catalog: BTreeMap<_, _> =
                    registry.gates.iter().map(|g| (g.id.clone(), g)).collect();
                let mut selected: BTreeSet<_> = members.iter().cloned().collect();
                loop {
                    let before = selected.len();
                    for id in selected.clone() {
                        selected.extend(catalog[&id].requires.iter().cloned());
                    }
                    if before == selected.len() {
                        break;
                    }
                }
                let mut done = BTreeSet::new();
                let mut gates = Vec::new();
                while done.len() != selected.len() {
                    let next = selected
                        .iter()
                        .find(|id| {
                            !done.contains(*id)
                                && catalog[*id].requires.iter().all(|dep| done.contains(dep))
                        })
                        .ok_or("unresolved CI job dependency")?
                        .clone();
                    done.insert(next.clone());
                    gates.push(next);
                }
                // CI job executes ALL declared members + deps, not path-filtered subset.
                Plan {
                    cadence: Cadence::Ci,
                    suites: selected
                        .iter()
                        .flat_map(|id| catalog[id].suites.iter().cloned())
                        .collect(),
                    gates,
                    jobs: BTreeMap::from([(job.to_owned(), true)]),
                }
            }
        };
        let gates = plan
            .gates
            .iter()
            .map(|id| {
                Ok((
                    GateId::new(id)?,
                    registry
                        .gates
                        .iter()
                        .find(|g| &g.id == id)
                        .ok_or("planned gate absent from catalog")?
                        .clone(),
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            plan,
            gates,
            registry_hash: evidence::canonical_registry_hash(&registry)?,
        })
    }
}

pub(crate) fn run(
    runtime: &mut impl Runtime,
    root: &ExecutionRoot,
    prepared: &PreparedPlan,
    metadata: Metadata,
    budgets: Budgets,
) -> Ledger {
    let started = runtime.now_ms();
    let capabilities = runtime.capabilities();
    let mut invalid = Vec::new();
    let expected = metadata.fingerprint.clone();
    let mut probe = |runtime: &mut dyn Runtime| match runtime.verify(root) {
        Ok(proof) if proof == expected => true,
        Ok(_) => {
            invalid.push("source/private snapshot identity changed".into());
            false
        }
        Err(error) => {
            invalid.push(format!("snapshot verification: {error}"));
            false
        }
    };
    let initial_ok = probe(runtime);
    let mut results: Vec<GateResult> = Vec::new();
    let mut identity_ok = initial_ok;
    for (id, gate) in &prepared.gates {
        let mut result = GateResult {
            gate: id.clone(),
            verdict: Verdict::Skipped,
            duration_ms: 0,
            reason: String::new(),
            blocked_by: Vec::new(),
            unavailable: Vec::new(),
            log: None,
            blocks_cadence: gate.blocks.contains(&prepared.plan.cadence),
        };
        let elapsed = Duration::from_millis(runtime.now_ms().saturating_sub(started));
        result.blocked_by = gate
            .requires
            .iter()
            .filter(|dep| {
                !results
                    .iter()
                    .any(|r| r.gate.as_str() == dep.as_str() && r.verdict == Verdict::Pass)
            })
            .cloned()
            .collect();
        result.unavailable = gate
            .capabilities
            .iter()
            .filter(|cap| {
                !matches!(
                    capabilities.get(cap),
                    Some(CapabilityState::Available { .. })
                )
            })
            .copied()
            .collect();
        if !identity_ok {
            result.reason = "invalid snapshot evidence; execution stopped".into();
        } else if runtime.cancelled() {
            result.reason = "human cancellation".into();
        } else if !result.blocked_by.is_empty() {
            result.reason = "dependency not PASS".into();
        } else if !result.unavailable.is_empty() {
            result.verdict = Verdict::Unavailable;
            result.reason = "required capability unavailable (see limits)".into();
        } else if elapsed >= budgets.total || budgets.per_gate_max.is_zero() {
            result.reason = "runner budget exhausted; selected gate remains incomplete".into();
        } else {
            identity_ok = probe(runtime);
            if identity_ok {
                // Verification consumes the same total wall budget.
                let start = runtime.now_ms();
                let remaining = budgets
                    .total
                    .saturating_sub(Duration::from_millis(start.saturating_sub(started)));
                let deadline = Duration::from_secs(u64::from(gate.budget_s))
                    .min(budgets.per_gate_max)
                    .min(remaining);
                if deadline.is_zero() {
                    result.reason = "runner budget exhausted during verification".into();
                    results.push(result);
                    continue;
                }
                let receipt = runtime.make(root, id, deadline);
                result.duration_ms = runtime.now_ms().saturating_sub(start);
                result.verdict = match receipt.exit {
                    Exit::Success => Verdict::Pass,
                    Exit::Failed | Exit::Deadline => Verdict::Fail,
                    Exit::Cancelled => Verdict::Skipped,
                    Exit::StartError | Exit::MonitorError => Verdict::Unavailable,
                };
                result.reason = receipt.reason;
                result.log = receipt.log;
                // Missing successful output is not usable PASS evidence.
                if result.log.is_none() && result.verdict == Verdict::Pass {
                    result.verdict = Verdict::Unavailable;
                    result.reason = "successful output could not be retained".into();
                }
                // Even failure/cancellation must run BOTH endpoint checks.
                identity_ok = probe(runtime);
            } else {
                result.reason = "pre-gate snapshot verification failed".into();
            }
        }
        results.push(result);
    }
    let _ = probe(runtime);
    if prepared.gates.is_empty() {
        invalid.push("empty selected plan is not validation evidence".into());
    }
    let overall = if !invalid.is_empty() {
        Overall::Invalid
    } else if results
        .iter()
        .any(|r| matches!(r.verdict, Verdict::Unavailable | Verdict::Skipped))
    {
        Overall::Incomplete
    } else if results.iter().any(|r| r.verdict == Verdict::Fail) {
        Overall::Fail
    } else {
        Overall::Pass
    };
    Ledger {
        schema: 1,
        authoritative: false,
        metadata,
        canonical_registry_hash: prepared.registry_hash.clone(),
        cadence: prepared.plan.cadence,
        suites: prepared.plan.suites.clone(),
        jobs: prepared.plan.jobs.clone(),
        capabilities,
        budgets,
        results,
        overall,
        invalid_reasons: invalid,
        duration_ms: runtime.now_ms().saturating_sub(started),
    }
}
