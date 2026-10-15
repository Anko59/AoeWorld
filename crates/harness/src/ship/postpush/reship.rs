//! Read-only reship notices for `make next`, including refusals already posted by the carry.
use super::{Result, api, comments, short};
use std::path::Path;

pub(in crate::ship) fn print(root: &Path, repository: &str) -> Result<()> {
    let pages = api(
        root,
        &[
            "--paginate",
            "--slurp",
            &format!("repos/{repository}/pulls?state=open&base=dev&per_page=100"),
        ],
    )?;
    for page in pages
        .as_array()
        .ok_or("pull request listing is not an array")?
    {
        let pulls = page
            .as_array()
            .map(|a| a.as_slice())
            .unwrap_or(std::slice::from_ref(page));
        for pull in pulls {
            if pull["auto_merge"].is_null() {
                continue;
            }
            let number = pull["number"]
                .as_u64()
                .ok_or("pull request has no number")?;
            let head = pull
                .pointer("/head/sha")
                .and_then(|v| v.as_str())
                .ok_or("pull request has no head")?;
            let refusal = format!("`harness/review` was not carried to {}: ", short(head));
            let success = format!("`harness/review` was carried to {}: ", short(head));
            let notes = comments(root, repository, number)?;
            let latest = notes
                .iter()
                .filter(|c| c.body.starts_with(&refusal) || c.body.starts_with(&success))
                .max_by_key(|c| c.id);
            if let Some(reason) = latest.and_then(|c| c.body.strip_prefix(&refusal)) {
                let reason = reason
                    .strip_suffix(". Ship again for a review of the change.")
                    .unwrap_or(reason);
                let title = pull["title"].as_str().unwrap_or("");
                let url = pull["html_url"].as_str().unwrap_or("");
                println!("#{number} {title} — reship: {reason}\n{url}");
            }
        }
    }
    Ok(())
}
