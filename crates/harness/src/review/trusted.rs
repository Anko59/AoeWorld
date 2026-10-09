//! Review inputs come from origin/dev, so a branch never rewrites its own
//! criteria. A review pins one origin/dev commit before it reads anything, so
//! a fetch while it runs changes neither its model, its prompts nor its policy
//! fingerprint.
use crate::ship::git;
use std::{cell::RefCell, path::Path};

thread_local! {
    static PINNED: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Unpins when the review ends, however it ends.
pub(super) struct Pin(());

impl Drop for Pin {
    fn drop(&mut self) {
        PINNED.with(|pinned| *pinned.borrow_mut() = None);
    }
}

/// The origin/dev commit the judge was built from, exported by
/// `.agents/hooks/harness.sh`.
pub(super) const JUDGE_REV: &str = "AOE_JUDGE_REV";

/// Resolve the policy commit once and read every trusted input from it until
/// the returned guard drops: the judge's own revision when it is on
/// origin/dev, otherwise origin/dev as it stands now.
pub(super) fn pin(root: &Path, judge: Option<&str>) -> Result<(Pin, String), String> {
    let commit = match judge.map(str::trim).filter(|judge| !judge.is_empty()) {
        Some(judge) => {
            let commit = git::git(
                root,
                &[
                    "rev-parse",
                    "--verify",
                    "--quiet",
                    &format!("{judge}^{{commit}}"),
                ],
            )
            .map_err(|_| format!("{JUDGE_REV} {judge} is not a commit"))?;
            // Never a branch's own commit: a branch must not set its criteria.
            git::git(
                root,
                &[
                    "merge-base",
                    "--is-ancestor",
                    &commit,
                    "refs/remotes/origin/dev",
                ],
            )
            .map_err(|_| format!("{JUDGE_REV} {commit} is not on origin/dev"))?;
            commit
        }
        None => git::git(
            root,
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                "refs/remotes/origin/dev^{commit}",
            ],
        )
        .map_err(|_| "origin/dev is missing: fetch it first".to_owned())?,
    };
    PINNED.with(|pinned| *pinned.borrow_mut() = Some(commit.clone()));
    Ok((Pin(()), commit))
}

/// A review input from the pinned commit (origin/dev when nothing is
/// pinned); the working tree only while dev does not have it yet (bootstrap).
pub(crate) fn trusted(root: &Path, relative: &str) -> Result<String, String> {
    let revision = PINNED
        .with(|pinned| pinned.borrow().clone())
        .unwrap_or_else(|| "refs/remotes/origin/dev".to_owned());
    match git::git(root, &["show", &format!("{revision}:{relative}")]) {
        Ok(text) => Ok(text),
        Err(_) => {
            std::fs::read_to_string(root.join(relative)).map_err(|e| format!("{relative}: {e}"))
        }
    }
}
