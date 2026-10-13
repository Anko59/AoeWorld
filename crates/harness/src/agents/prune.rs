//! Keeps the hook judge directory small. `.agents/hooks/harness.sh` builds one
//! `<revision>/aoe-harness` (or `bootstrap-<tree>/aoe-harness`) per origin/dev
//! revision under `<git common dir>/aoe-agent-hook/` and never deletes any; after
//! it selects a judge it asks that judge to prune the rest here.
use std::{
    fs,
    path::Path,
    time::{Duration, SystemTime},
};

/// A judge younger than this may be mid-build or about to be executed by
/// another session.
pub(crate) const YOUNG: Duration = Duration::from_secs(10 * 60);
const BINARY: &str = "aoe-harness";
const HOME: &str = "aoe-agent-hook";

/// The names `harness.sh` creates: a revision, or `bootstrap-` and a tree.
fn judge_name(name: &str) -> bool {
    let hash = name.strip_prefix("bootstrap-").unwrap_or(name);
    hash.len() == 40 && hash.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Removes judge directories under `home` except `keep` (the judge in use), the
/// newest other judge, and any younger than [`YOUNG`]. Best effort: failures,
/// including a concurrent pruner winning the race, are ignored. Returns how many
/// directories this call removed. A `home` not named `aoe-agent-hook`, or a
/// `keep` that is not a judge directory holding a binary, deletes nothing.
pub(crate) fn prune(home: &Path, keep: &str, now: SystemTime) -> usize {
    if home.file_name().is_none_or(|n| n != HOME)
        || !judge_name(keep)
        || !home.join(keep).join(BINARY).is_file()
    {
        return 0;
    }
    let Ok(entries) = fs::read_dir(home) else {
        return 0;
    };
    let mut others: Vec<(SystemTime, std::path::PathBuf)> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name();
            let name = name.to_str().filter(|n| judge_name(n) && *n != keep)?;
            let dir = home.join(name);
            // A symlink or stray file with a judge's name is not ours to remove.
            fs::symlink_metadata(&dir).ok()?.is_dir().then_some(())?;
            let built = fs::metadata(dir.join(BINARY))
                .and_then(|m| m.modified())
                .ok()?;
            Some((built, dir))
        })
        .collect();
    others.sort_by(|a, b| b.0.cmp(&a.0));
    others
        .into_iter()
        .skip(1)
        .filter(|(built, _)| now.duration_since(*built).is_ok_and(|age| age >= YOUNG))
        .filter(|(_, dir)| fs::remove_dir_all(dir).is_ok())
        .count()
}
