//! Read-only reship notices for `make next`, including refusals already posted by the carry,
//! and conflicts the PR updater could not merge into the current head.
use super::{Comment, Result, api, comments, short, update};
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
    // Conflicts come first: the updater left those pull requests unmerged, so nothing else moves them.
    let mut conflicts = Vec::new();
    let mut refusals = Vec::new();
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
            let title = pull["title"].as_str().unwrap_or("");
            let url = pull["html_url"].as_str().unwrap_or("");
            if let Some(files) = conflict_files(&notes, head) {
                conflicts.push(format!(
                    "#{number} {title} — reship: conflicts with dev in {files}\n{url}"
                ));
            }
            let latest = notes
                .iter()
                .filter(|c| c.body.starts_with(&refusal) || c.body.starts_with(&success))
                .max_by_key(|c| c.id);
            if let Some(reason) = latest.and_then(|c| c.body.strip_prefix(&refusal)) {
                let reason = reason
                    .strip_suffix(". Ship again for a review of the change.")
                    .unwrap_or(reason);
                refusals.push(format!("#{number} {title} — reship: {reason}\n{url}"));
            }
        }
    }
    for line in conflicts.iter().chain(&refusals) {
        println!("{line}");
    }
    Ok(())
}

/// The files of the newest conflict comment, if it is for `head`: a later push leaves an older one behind.
fn conflict_files(notes: &[Comment], head: &str) -> Option<String> {
    let tag = update::conflict_tag(head);
    let latest = notes
        .iter()
        .filter(|c| c.body.starts_with(&tag))
        .max_by_key(|c| c.id)?;
    Some(
        update::conflict_list(&latest.body)
            .unwrap_or("files not named in the comment")
            .to_owned(),
    )
}
