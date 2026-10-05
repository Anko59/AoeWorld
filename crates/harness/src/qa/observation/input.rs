use super::{REPORT_BYTES, Report, Result};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::qa) struct ExpectedSource {
    pub(in crate::qa) candidate_content_witness_digest: String,
    pub(in crate::qa) review_subject_digest: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Wrapped {
    version: u16,
    report: Report,
    expected_source: ExpectedSource,
}
pub(super) fn parse(bytes: &[u8], allow_source: bool) -> Result<(Report, Option<ExpectedSource>)> {
    let value = crate::input_json::parse(bytes, REPORT_BYTES)?;
    if value.get("version").and_then(serde_json::Value::as_u64) != Some(2) {
        return Ok((serde_json::from_value(value)?, None));
    }
    let wrapped: Wrapped = serde_json::from_value(value)?;
    if wrapped.version != 2 {
        return Err("unsupported QA source wrapper version".into());
    }
    for digest in [
        &wrapped.expected_source.candidate_content_witness_digest,
        &wrapped.expected_source.review_subject_digest,
    ] {
        if digest.len() != 64
            || !digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err("QA expected source digest must be full lowercase BLAKE3 hex".into());
        }
    }
    if !allow_source {
        return Err("QA v2 requires retained immutable source context".into());
    }
    Ok((wrapped.report, Some(wrapped.expected_source)))
}
