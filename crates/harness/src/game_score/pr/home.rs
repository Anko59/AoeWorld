//! The shared home of the game score: one folder for every worktree of a checkout, beside
//! `aoe-cargo` in the common Git directory. The Makefile already mounts that directory in every
//! container, so the map and the results are visible where the game score runs and never in Git.
//!
//! ```text
//! aoe-game-score/
//!   packages/          the map package (`make game-map`, `make game-score`)
//!   results/<key>.json one entry of the results cache per game state (see `key`)
//!   base/              the scratch worktree the merge base is built and played in
//! ```
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

/// Names another home (tests, a machine with the common directory on a slow disk).
pub(crate) const ENV: &str = "AOE_GAME_SCORE_HOME";
const FOLDER: &str = "aoe-game-score";

/// The home of the checkout at `root`: the one `AOE_GAME_SCORE_HOME` names, else the one beside
/// `aoe-cargo` in the common Git directory.
pub(crate) fn directory(root: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Some(named) = std::env::var_os(ENV).filter(|named| !named.is_empty()) {
        return Ok(PathBuf::from(named));
    }
    Ok(in_directory(None, &crate::hooks::common_directory(root)?))
}

/// `named` if given, else `aoe-game-score` in the common Git directory.
fn in_directory(named: Option<OsString>, common: &Path) -> PathBuf {
    match named.filter(|name| !name.is_empty()) {
        Some(name) => PathBuf::from(name),
        None => common.join(FOLDER),
    }
}

pub(crate) fn packages(home: &Path) -> PathBuf {
    home.join("packages")
}

pub(super) fn results(home: &Path) -> PathBuf {
    home.join("results")
}

pub(super) fn base_checkout(home: &Path) -> PathBuf {
    home.join("base")
}

/// Where CI keeps the package: `.github/actions/game-map-restore` restores it there and the
/// `game-map` workflow saves it from there, so a CI run keeps that place whatever the home is.
const CI_PACKAGES: &str = ".cache/game-map/packages";

/// The package directory when no flag names one: the home's, in CI the folder its workflows
/// cache, and outside a Git checkout a folder of the current directory. Read at run time, never
/// when the command line is built.
pub(crate) fn default_packages() -> PathBuf {
    let ci = std::env::var_os("GITHUB_ACTIONS").is_some_and(|value| !value.is_empty());
    packages_default(ci, directory(Path::new(".")).ok())
}

fn packages_default(ci: bool, home: Option<PathBuf>) -> PathBuf {
    match home {
        Some(home) if !ci => packages(&home),
        _ if ci => PathBuf::from(CI_PACKAGES),
        _ => PathBuf::from(".cache/game-score/packages"),
    }
}

#[cfg(test)]
#[path = "tests/home.rs"]
mod tests;
