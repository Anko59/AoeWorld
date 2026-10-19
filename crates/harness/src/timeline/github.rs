use super::Result;
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Pr {
    pub(super) number: u64,
    pub(super) head_ref_name: String,
    pub(super) head_ref_oid: String,
    pub(super) created_at: f64,
    pub(super) merged_at: f64,
    #[serde(default)]
    pub(super) runs: Vec<Run>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Run {
    pub(super) database_id: u64,
    pub(super) name: String,
    pub(super) head_sha: String,
    pub(super) started_at: Option<f64>,
    pub(super) updated_at: Option<f64>,
    pub(super) status: String,
    pub(super) conclusion: String,
}

fn query<T: serde::de::DeserializeOwned>(root: &Path, args: &[&str]) -> Result<Vec<T>> {
    Ok(serde_json::from_str(&crate::ship::gh(root, args)?)?)
}

/// Two bounded, read-only queries rather than an API call for every PR/run.
/// Query failures preserve local measurements and name the missing evidence.
pub(super) fn read(root: &Path, since: f64) -> (Vec<Pr>, Vec<String>) {
    let mut warnings = Vec::new();
    let repository = match crate::ship::git::origin_repository(root) {
        Ok(repository) => repository,
        Err(e) => return (Vec::new(), vec![format!("GitHub unavailable: {e}")]),
    };
    let date = crate::ship::lock::utc(since as u64)[..10].to_owned();
    let mut prs = match query::<Pr>(
        root,
        &[
            "pr",
            "list",
            "--repo",
            &repository,
            "--state",
            "merged",
            "--search",
            &format!("merged:>={date}"),
            "--limit",
            "500",
            "--json",
            "number,headRefName,headRefOid,createdAt,mergedAt",
            "--jq",
            "map(.createdAt |= fromdateiso8601 | .mergedAt |= fromdateiso8601)",
        ],
    ) {
        Ok(prs) => prs,
        Err(e) => return (Vec::new(), vec![format!("merged PRs unavailable: {e}")]),
    };
    if prs.len() == 500 {
        warnings.push("merged PR query reached 500; history may be truncated".into());
    }
    if prs.is_empty() {
        return (prs, warnings);
    }
    let runs = match query::<Run>(
        root,
        &[
            "run",
            "list",
            "--repo",
            &repository,
            "--created",
            &format!(">={date}"),
            "--limit",
            "2000",
            "--json",
            "databaseId,name,headSha,startedAt,updatedAt,status,conclusion",
            "--jq",
            "map(.startedAt |= (if . == null or . == \"\" then null else fromdateiso8601 end) | .updatedAt |= (if . == null or . == \"\" then null else fromdateiso8601 end))",
        ],
    ) {
        Ok(runs) => runs,
        Err(e) => {
            warnings.push(format!("CI runs unavailable: {e}"));
            Vec::new()
        }
    };
    if runs.len() == 2000 {
        warnings.push("CI query reached 2000 runs; CI history may be truncated".into());
    }
    for pr in &mut prs {
        pr.runs = runs
            .iter()
            .filter(|r| r.head_sha == pr.head_ref_oid)
            .cloned()
            .collect();
        pr.runs.sort_by(|a, b| {
            a.started_at
                .unwrap_or(0.)
                .total_cmp(&b.started_at.unwrap_or(0.))
        });
    }
    prs.sort_by(|a, b| a.merged_at.total_cmp(&b.merged_at));
    (prs, warnings)
}
