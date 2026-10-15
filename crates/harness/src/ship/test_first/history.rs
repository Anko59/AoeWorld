//! Attribute history by ancestry and merge trees, independent of commit identity.
use super::{git, test_marker};
use std::{collections::BTreeSet, path::Path};

pub(super) struct Commit {
    pub sha: String,
    /// Automatic merge tree for a changed base/dev merge, first parent otherwise.
    pub against: Option<String>,
    pub merge: bool,
}

pub(super) fn own(root: &Path, base: &str, head: &str) -> git::Result<Vec<Commit>> {
    let selected = crate::review::base::reference(&crate::review::base::current());
    let mut boundaries = vec![base.to_owned()];
    for reference in [selected.as_str(), "refs/remotes/origin/dev"] {
        if let Ok(commit) = git::git(
            root,
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("{reference}^{{commit}}"),
            ],
        ) && !boundaries.contains(&commit)
        {
            boundaries.push(commit);
        }
    }
    let mut args = vec!["rev-list"];
    args.extend(boundaries.iter().map(String::as_str));
    let ancestors = git::git(root, &args)?;
    let ancestors: BTreeSet<_> = ancestors.lines().collect();
    let range = format!("{base}..{head}");
    let mut args = vec![
        "rev-list",
        "--reverse",
        "--topo-order",
        "--parents",
        &range,
        "--not",
    ];
    args.extend(boundaries.iter().map(String::as_str));
    let list = git::git(root, &args)?;
    let mut commits = Vec::new();
    for line in list.lines() {
        let ids: Vec<_> = line.split_whitespace().collect();
        let sha = ids[0];
        let parents = &ids[1..];
        let mut against = parents.first().map(|parent| (*parent).to_owned());
        if parents.len() == 2 && ancestors.contains(parents[1]) {
            // Only the automatic merge is incoming work. Extra edits belong
            // to the PR, even when GitHub authored the merge. A conflict (or
            // unsupported multi-parent merge) keeps the first-parent diff.
            if let Ok(automatic) = git::git(
                root,
                &["merge-tree", "--write-tree", parents[0], parents[1]],
            ) {
                if automatic == git::git(root, &["rev-parse", &format!("{sha}^{{tree}}")])? {
                    continue;
                }
                against = Some(automatic);
            }
        }
        commits.push(Commit {
            sha: sha.to_owned(),
            against,
            merge: parents.len() > 1,
        });
    }
    Ok(commits)
}

/// Count surviving test attributes introduced by the PR before its first
/// implementation. Blame avoids counting incoming dev tests or counting a
/// side branch's tests again when its own merge is retained.
pub(super) fn tests_before(
    root: &Path,
    base: &str,
    before: &str,
    own: &BTreeSet<String>,
) -> git::Result<usize> {
    let paths = git::git_raw(
        root,
        &[
            "diff",
            "--name-only",
            "-z",
            "--diff-filter=AMT",
            "--no-renames",
            base,
            before,
            "--",
            "crates/",
            ":(exclude)crates/harness/",
        ],
    )?;
    let mut count = 0;
    for path in paths.split('\0').filter(|path| !path.is_empty()) {
        let blame = git::git_raw(root, &["blame", "--line-porcelain", before, "--", path])?;
        let mut owned = false;
        for line in blame.lines() {
            if let Some(text) = line.strip_prefix('\t') {
                count += usize::from(owned && test_marker(text));
                continue;
            }
            let fields: Vec<_> = line.split_whitespace().collect();
            if fields.len() >= 3
                && matches!(fields[0].len(), 40 | 64)
                && fields[0].bytes().all(|byte| byte.is_ascii_hexdigit())
                && fields[1].parse::<usize>().is_ok()
                && fields[2].parse::<usize>().is_ok()
            {
                owned = own.contains(fields[0]);
            }
        }
    }
    Ok(count)
}
