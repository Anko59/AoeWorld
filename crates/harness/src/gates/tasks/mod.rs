//! Local portable planning: roles/prompts and provider descriptors are not permissions.
mod cli;
mod io;
pub(crate) use cli::{Options, execute};
mod adapters;
mod catalog;
mod integrity;
mod task;
#[cfg(test)]
mod tests;
use crate::gates::registry::{Cadence, Plan, Registry};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, error::Error};
type Result<T> = std::result::Result<T, Box<dyn Error>>;
use catalog::{Catalog, Role};
use task::{ContextPacket, Task};

#[derive(Debug, Serialize)]
struct TaskPlan {
    version: u16,
    task: Task,
    context: ContextPacket,
    registry_hash: String,
    catalog_hash: String,
    required: Plan,
    handoff: Plan,
    fixture_namespace: String,
    role_guidance: catalog::RoleSpec,
    semantic_handoffs: Vec<task::Handoff>,
    integrity: integrity::Finding,
    adapter: adapters::Descriptor,
    authoritative: bool,
    limits: Vec<String>,
}

/// Caller supplies immutable resolved diff paths/hunks, never task-JSON path claims.
/// Candidate catalog/policy planning is explicitly non-authoritative local feedback.
fn plan(
    task: Task,
    registry: &Registry,
    catalog: &Catalog,
    observed_paths: &[String],
    hunks: &[integrity::Hunk],
    runtime: adapters::Observation,
) -> Result<TaskPlan> {
    let hash = registry.fingerprint()?;
    task.validate(&hash)?;
    let catalog_hash = catalog.fingerprint()?;
    let classification = registry.classify(observed_paths);
    let required = registry.plan(Cadence::Pr, &classification.suites)?;
    let handoff = registry.plan(Cadence::Preflight, &BTreeSet::from(["everything".into()]))?;
    let role_guidance = catalog.role(task.role)?.clone();
    let integrity = integrity::inspect(registry, observed_paths, hunks)?;
    let context = ContextPacket::new(&task, &role_guidance)?;
    let adapter = adapters::describe(task.provider, runtime)?;
    let fixture_namespace = format!("task-fixtures/{}", task.id);
    let semantic_handoffs = task.semantic_handoffs();
    Ok(TaskPlan { version: 1, task, context, registry_hash: hash, catalog_hash,
        required, handoff, fixture_namespace, role_guidance, semantic_handoffs,
        integrity, adapter, authoritative: false,
        limits: vec!["Candidate catalog is local bootstrap guidance, not protected policy or authenticated role identity".into(),
            "Task state and transition suggestions are unverified JSON, never review/test evidence".into(),
            "Artifact and fixture namespaces are logical names, not filesystem permissions or hidden-test isolation".into(),
            "No live interception, provider launch, arbitrary shell, publication, approval or merge is performed".into(),
            "Full preflight is mandatory; selected unavailable checks remain incomplete".into(),
            "Test-integrity heuristics mandate independent review; absence never establishes quality PASS".into()] })
}
fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}
fn hex_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
fn oid(value: &str) -> bool {
    matches!(value.len(), 40 | 64)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
fn relative(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 240
        && !value.contains(['\\', ':'])
        && !value.chars().any(char::is_control)
        && value.split('/').all(|part| {
            !part.is_empty() && part != "." && part != ".." && !part.eq_ignore_ascii_case(".git")
        })
}
