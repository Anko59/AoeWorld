//! Exact logical review subjects, never authenticated review or approval.
mod cli;
mod schema;
#[cfg(test)]
mod tests;
use super::{
    contracts,
    registry::{Cadence, Plan, Registry},
    scopes::{Kind, Snapshot},
};
pub(crate) use cli::{Options, execute};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
};
type Result<T> = std::result::Result<T, Box<dyn Error>>;
const MAX_BYTES: usize = 4 * 1024 * 1024;
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
enum Scope {
    Commit {
        base: String,
        candidate: String,
    },
    Index {
        base: String,
        captured_source_head: String,
        pending_tree: String,
    },
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct Subject {
    schema: u16,
    algorithm: String,
    repository_anchor: String,
    scope: Scope,
    base_content: Value,
    candidate_content: Value,
    contracts: Value,
    canonical_registry_hash: String,
    affected_paths: Vec<String>,
    required_pr_plan: Plan,
    mandatory_preflight_plan: Plan,
}
/// Generated from retained snapshots using the exact review materializer, not claims.
#[derive(Debug, Eq, PartialEq, Serialize)]
pub(crate) struct SourceSummary {
    pub(crate) schema: u16,
    pub(crate) review_algorithm: &'static str,
    pub(crate) review_subject_digest: String,
    pub(crate) candidate_content_witness_digest: String,
    pub(crate) base_content_witness_digest: String,
    pub(crate) scope: Value,
}
pub(crate) fn source_summary(base: &Snapshot, candidate: &Snapshot) -> Result<SourceSummary> {
    let subject = materialize(base, candidate)?;
    let result = SourceSummary {
        schema: 1,
        review_algorithm: "blake3:aoeworld-review-subject-v1",
        review_subject_digest: digest(&subject)?,
        candidate_content_witness_digest: subject.candidate_content["digest"]
            .as_str()
            .ok_or("generated candidate witness digest missing")?
            .into(),
        base_content_witness_digest: subject.base_content["digest"]
            .as_str()
            .ok_or("generated base witness digest missing")?
            .into(),
        scope: serde_json::to_value(&subject.scope)?,
    };
    recheck(base, candidate, &subject)?;
    Ok(result)
}
fn files(witness: &Value) -> Result<BTreeMap<String, Value>> {
    let mut files = BTreeMap::new();
    for item in witness["files"]
        .as_array()
        .ok_or("logical file inventory missing")?
    {
        let path = item["path"].as_str().ok_or("logical file path missing")?;
        if files.insert(path.into(), item.clone()).is_some() {
            return Err("duplicate logical file path".into());
        }
    }
    Ok(files)
}
fn affected(base: &Value, candidate: &Value) -> Result<Vec<String>> {
    let base = files(base)?;
    let candidate = files(candidate)?;
    let paths: BTreeSet<_> = base.keys().chain(candidate.keys()).cloned().collect();
    Ok(paths
        .into_iter()
        .filter(|path| base.get(path) != candidate.get(path))
        .collect())
}
fn verify_bindings(report: &Value, candidate: &Value) -> Result<()> {
    let files = files(candidate)?;
    for record in report["bindings"]
        .as_array()
        .ok_or("checked contract bindings missing")?
    {
        let binding = &record["binding"];
        let path = binding["path"]
            .as_str()
            .ok_or("checked binding path missing")?;
        let file = files
            .get(path)
            .ok_or("checked binding outside logical witness")?;
        if binding["file_blake3"] != file["raw_blake3"]
            || binding["file_blake3"].as_str().is_none()
            || binding["line"].as_u64().is_none_or(|line| line == 0)
        {
            return Err("checked binding does not match actual logical file witness".into());
        }
    }
    Ok(())
}
fn materialize(base: &Snapshot, candidate: &Snapshot) -> Result<Subject> {
    let base_content = serde_json::to_value(base.content_witness()?)?;
    let candidate_content = serde_json::to_value(candidate.content_witness()?)?;
    let base_oid = match &base.identity.kind {
        Kind::Commit(oid) => oid.clone(),
        _ => return Err("review base must be an independent full commit".into()),
    };
    let scope = match &candidate.identity.kind {
        Kind::Commit(oid) => Scope::Commit {
            base: base_oid,
            candidate: oid.clone(),
        },
        Kind::Index => Scope::Index {
            base: base_oid,
            captured_source_head: candidate.identity.source_head.clone(),
            pending_tree: candidate
                .identity
                .tree
                .clone()
                .ok_or("pending tree missing")?,
        },
        Kind::Working => return Err("working source cannot be an exact review subject".into()),
    };
    let report = candidate.run_checked(contracts::check)?;
    verify_bindings(&report, &candidate_content)?;
    let registry = candidate.run_checked(Registry::load)?;
    let registry_hash = registry.fingerprint()?;
    let contract_digest = report["contracts_blake3"]
        .as_str()
        .ok_or("checked contract digest missing")?;
    if report["contracts_hash_algorithm"] != "blake3:aoeworld-contracts-raw-v1"
        || contract_digest.len() != 64
        || !contract_digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        || report["registry_hash"] != registry_hash
    {
        return Err("checked contract digest/registry witness mismatch".into());
    }
    let affected_paths = affected(&base_content, &candidate_content)?;
    let classified = registry.classify(&affected_paths);
    let subject = Subject {
        schema: 1,
        algorithm: "blake3:aoeworld-review-subject-v1".into(),
        repository_anchor: "UNAVAILABLE".into(),
        scope,
        base_content,
        candidate_content,
        contracts: json!({"algorithm":report["contracts_hash_algorithm"],"digest":report["contracts_blake3"],"bindings":report["bindings"]}),
        canonical_registry_hash: registry.fingerprint()?,
        affected_paths,
        required_pr_plan: registry.plan(Cadence::Pr, &classified.suites)?,
        mandatory_preflight_plan: registry
            .plan(Cadence::Preflight, &BTreeSet::from(["everything".into()]))?,
    };
    recheck(base, candidate, &subject)?;
    encoded(&subject)?;
    Ok(subject)
}
fn recheck(base: &Snapshot, candidate: &Snapshot, subject: &Subject) -> Result<()> {
    if serde_json::to_value(base.content_witness()?)? != subject.base_content
        || serde_json::to_value(candidate.content_witness()?)? != subject.candidate_content
    {
        return Err("logical review content changed before publication".into());
    }
    base.run_checked(|_| Ok(()))?;
    candidate.run_checked(|_| Ok(()))?;
    Ok(())
}
fn encoded(subject: &Subject) -> Result<Vec<u8>> {
    let bytes = serde_json::to_vec(subject)?;
    if bytes.len() > MAX_BYTES {
        return Err("complete review subject exceeds4MiB".into());
    }
    Ok(bytes)
}
fn digest(subject: &Subject) -> Result<String> {
    let mut hash = blake3::Hasher::new();
    hash.update(b"aoeworld:review-subject:v1\0");
    hash.update(&encoded(subject)?);
    Ok(hash.finalize().to_hex().to_string())
}
fn assessment(subject: &Subject, review: Option<&schema::Review>) -> Result<Value> {
    let status = if let Some(review) = review {
        if review.subject != serde_json::to_value(subject)? {
            return Err("submitted review subject does not match generated logical subject".into());
        }
        "EXACT_SUBJECT_MATCH_REVIEW_NOT_AUTHENTICATED"
    } else {
        "SUBJECT_MATERIALIZED_NON_AUTHORITATIVE"
    };
    Ok(
        json!({"schema":1,"status":status,"authoritative":false,"subject_blake3":digest(subject)?,"independent_review":"NOT_ASSESSED","semantic_case_execution":"NOT_ASSESSED","authenticated_verdict":"UNAVAILABLE","limits":["candidate registry plans are feedback, not protected judge minimum policy","subject or notes hashes do not authenticate roles, independence, approval or submission","local Git/filesystem operations are not hard wall-supervised; endpoint checks cannot detect every reverted race","source merges and host daemon access do not qualify trusted service deployment"]}),
    )
}
