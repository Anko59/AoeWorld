//! Review inputs come from origin/dev, so a branch never rewrites its own
//! criteria. A review pins the origin/dev commit it starts from, so a fetch
//! while it runs changes neither its prompts nor its policy fingerprint.
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

/// Resolve origin/dev once and read every trusted input from that commit
/// until the returned guard drops. Without origin/dev nothing is pinned.
pub(super) fn pin(root: &Path) -> (Pin, Option<String>) {
    let commit = git::git(
        root,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            "refs/remotes/origin/dev^{commit}",
        ],
    )
    .ok();
    PINNED.with(|pinned| pinned.borrow_mut().clone_from(&commit));
    (Pin(()), commit)
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
