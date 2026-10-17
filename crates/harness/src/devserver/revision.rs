//! Which revision the dev server should run, and the fingerprint of the map
//! code that decides whether its prepared maps are still good.
use crate::ship::git::git;
use std::path::Path;

type Result<T> = std::result::Result<T, String>;

/// Everything a prepared map's content depends on: the map and worker
/// crates, their shared types, how a request becomes a plan, and the lock file.
const MAP_CODE: &[&str] = &[
    "crates/map",
    "crates/geodata",
    "crates/core",
    "crates/server/src/map_worker.rs",
    "crates/server/src/map_worker",
    "crates/server/src/map_jobs/preparation.rs",
    "crates/server/src/map_store.rs",
    "crates/server/src/map_store",
    "Cargo.lock",
    "rust-toolchain.toml",
];

/// `AOE_DEV_SERVER_REV` (any commit-ish), else the local `origin/dev`.
pub(crate) fn expected(root: &Path, requested: Option<&str>) -> Result<String> {
    let spec = requested.unwrap_or("refs/remotes/origin/dev");
    let sha = git(
        root,
        &["rev-parse", "--verify", &format!("{spec}^{{commit}}")],
    )?;
    if sha.len() == 40 && sha.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(sha)
    } else {
        Err(format!("`{spec}` is not a commit"))
    }
}

/// Bring `origin/dev` up to date; a machine without a network keeps the ref it has.
pub(crate) fn fetch(root: &Path) -> Option<String> {
    git(root, &["fetch", "--quiet", "origin", "dev"]).err()
}

/// A short hash of the Git object ids of [`MAP_CODE`] at `revision`: equal
/// while none of that code changed, whatever else dev did.
pub(crate) fn fingerprint(root: &Path, revision: &str) -> Result<String> {
    let mut hasher = blake3::Hasher::new();
    for path in MAP_CODE {
        let id = git(root, &["rev-parse", &format!("{revision}:{path}")])
            .unwrap_or_else(|_| "absent".into());
        hasher.update(path.as_bytes());
        hasher.update(id.as_bytes());
    }
    Ok(hasher.finalize().to_hex()[..16].to_owned())
}
