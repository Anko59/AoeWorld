//! Exact generated logical source matching, never runtime or review authority.
use super::{
    Status,
    observation::{self, Observation, RetainedInputs},
};
use crate::gates::{
    review::{self, SourceSummary},
    scopes::{Kind, Snapshot},
};
use clap::{ArgGroup, Args};
use serde::Serialize;
use std::{
    error::Error,
    path::{Path, PathBuf},
};
type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Debug, Args)]
#[command(group(ArgGroup::new("qa_source_scope").args(["candidate", "index"]).multiple(false).requires("base")))]
pub(crate) struct Options {
    #[arg(default_value = "reports/qa/session.json")]
    pub(crate) file: PathBuf,
    #[arg(long, requires = "qa_source_scope")]
    pub(crate) base: Option<String>,
    #[arg(long)]
    pub(crate) candidate: Option<String>,
    #[arg(long)]
    pub(crate) index: bool,
}
#[derive(Debug, Serialize)]
struct SourceBinding {
    assessment: &'static str,
    authoritative: bool,
    summary: SourceSummary,
}
#[derive(Debug, Serialize)]
pub(crate) struct BoundObservation {
    schema: u16,
    assessment: &'static str,
    authoritative: bool,
    claimed_status: Status,
    source_identity: &'static str,
    served_build_binding: &'static str,
    independent_qa: &'static str,
    journey_execution: &'static str,
    observation: Observation,
    source_binding: SourceBinding,
}
fn full_oid(value: &str) -> Result<()> {
    if !matches!(value.len(), 40 | 64)
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("QA source requires full lowercase Git object IDs".into());
    }
    Ok(())
}
fn checked_match(inputs: &RetainedInputs, summary: &SourceSummary) -> Result<()> {
    let expected = inputs
        .expected_source
        .as_ref()
        .ok_or("QA immutable source arguments require a version-2 report")?;
    if expected.candidate_content_witness_digest != summary.candidate_content_witness_digest
        || expected.review_subject_digest != summary.review_subject_digest
    {
        return Err("QA expected source does not match generated immutable subject".into());
    }
    Ok(())
}
fn bind_retained(
    mut inputs: RetainedInputs,
    base: &Snapshot,
    candidate: &Snapshot,
    summary: SourceSummary,
) -> Result<BoundObservation> {
    checked_match(&inputs, &summary)?;
    inputs.recheck_all()?;
    if review::source_summary(base, candidate)? != summary {
        return Err("QA immutable source changed during evidence observation".into());
    }
    inputs.recheck_all()?;
    if base.content_witness()?.digest != summary.base_content_witness_digest
        || candidate.content_witness()?.digest != summary.candidate_content_witness_digest
    {
        return Err("QA source witness changed at final evidence endpoint".into());
    }
    Ok(BoundObservation {
        schema: 1,
        assessment: "SOURCE_AND_EVIDENCE_MATCH_OBSERVED_NON_AUTHORITATIVE",
        authoritative: false,
        claimed_status: inputs.observation.claimed_status,
        source_identity: "MEASURED_LOGICAL_CONTENT_NON_AUTHORITATIVE",
        served_build_binding: "UNAVAILABLE",
        independent_qa: "NOT_ASSESSED",
        journey_execution: "NOT_ASSESSED",
        observation: inputs.observation,
        source_binding: SourceBinding {
            assessment: "EXACT_GENERATED_SOURCE_MATCH_NON_AUTHORITATIVE",
            authoritative: false,
            summary,
        },
    })
}
fn bind_at(options: &Options, root: &Path, evidence_root: &Path) -> Result<BoundObservation> {
    let base_oid = options
        .base
        .as_deref()
        .ok_or("QA v2 requires an immutable base")?;
    full_oid(base_oid)?;
    let kind = match (&options.candidate, options.index) {
        (Some(candidate), false) => {
            full_oid(candidate)?;
            Kind::Commit(candidate.clone())
        }
        (None, true) => Kind::Index,
        _ => {
            return Err(
                "QA source requires exactly one candidate commit or intentional index".into(),
            );
        }
    };
    let base = Snapshot::prepare_independent(root, Kind::Commit(base_oid.into()))?;
    let candidate = match kind {
        Kind::Index => Snapshot::prepare(root, Kind::Index)?,
        other => Snapshot::prepare_independent(root, other)?,
    };
    // Original evidence FDs remain held through both actual summary passes and final endpoints.
    let summary = review::source_summary(&base, &candidate)?;
    let inputs = observation::retain_file_at(&options.file, evidence_root, true)?;
    bind_retained(inputs, &base, &candidate, summary)
}
pub(crate) fn execute(options: Options) -> Result<()> {
    if options.base.is_none() && options.candidate.is_none() && !options.index {
        return super::validate_file(&options.file);
    }
    let result = bind_at(&options, Path::new("."), Path::new("reports/qa"))?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
#[cfg(test)]
mod tests;
