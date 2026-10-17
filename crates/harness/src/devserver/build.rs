//! The dev server's build: `git archive` of one revision, built in the tool
//! image with the commands of `make dev`, the worker beside the server, and
//! the shared target directory trimmed afterwards.
use super::layout::Layout;
use crate::ship::compare::build::{build_app, docker_tools};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Written last: a tree without it is a half-made one and is thrown away.
const DONE: &str = "dev-server.built";

/// A built revision: the sources the server serves and its two executables.
pub(crate) struct Built {
    pub(crate) tree: PathBuf,
    pub(crate) server: PathBuf,
    pub(crate) worker: PathBuf,
}

impl Built {
    pub(crate) fn of(layout: &Layout, revision: &str) -> Self {
        let tree = layout.tree(revision);
        Self {
            server: tree.join("aoe-server.built"),
            worker: tree.join("aoe-map-worker.built"),
            tree,
        }
    }

    pub(crate) fn ready(&self) -> bool {
        self.tree.join(DONE).is_file() && self.server.is_file() && self.worker.is_file()
    }
}

/// The revision built under the shared home, building it when it is not.
pub(crate) fn ensure(layout: &Layout, root: &Path, revision: &str) -> Result<Built> {
    let built = Built::of(layout, revision);
    if built.ready() {
        return Ok(built);
    }
    // A half-unpacked or half-built tree is thrown away, never trusted.
    let _ = fs::remove_dir_all(&built.tree);
    fs::create_dir_all(&built.tree)?;
    export(root, revision, &built.tree)?;
    let target = layout.target();
    let cargo_home = root.join(".cache/cargo");
    fs::create_dir_all(&cargo_home)?;
    eprintln!(
        "dev-server: building {} (this takes minutes when cold)",
        &revision[..12]
    );
    build_app(&built.tree, &target, &cargo_home)?;
    eprintln!("dev-server: cargo build --release -p aoe-geodata --bin aoe-map-worker");
    docker_tools(
        &built.tree,
        &target,
        &cargo_home,
        &[
            "cargo",
            "build",
            "--locked",
            "--release",
            "-p",
            "aoe-geodata",
            "--bin",
            "aoe-map-worker",
        ],
    )?;
    fs::copy(target.join("release/aoe-server"), &built.server)?;
    fs::copy(target.join("release/aoe-map-worker"), &built.worker)?;
    trim(&target);
    fs::write(built.tree.join(DONE), revision)?;
    Ok(built)
}

fn export(root: &Path, revision: &str, tree: &Path) -> Result<()> {
    let mut archive = Command::new("git")
        .args(["archive", "--format=tar", revision])
        .current_dir(root)
        .stdout(Stdio::piped())
        .spawn()?;
    let tar = Command::new("tar")
        .args(["-x", "-f", "-", "-C"])
        .arg(tree)
        .stdin(archive.stdout.take().ok_or("git archive has no output")?)
        .status()?;
    if archive.wait()?.success() && tar.success() {
        Ok(())
    } else {
        Err(format!("could not export {revision} to {}", tree.display()).into())
    }
}

/// Incremental-compilation state is the bulk of a finished target directory
/// and is useless to a release build that is not repeated in place; the
/// dependency artifacts stay, so the next revision builds in seconds.
pub(crate) fn trim(target: &Path) {
    let Ok(profiles) = fs::read_dir(target) else {
        return;
    };
    for profile in profiles.flatten() {
        let _ = fs::remove_dir_all(profile.path().join("incremental"));
        if let Ok(inner) = fs::read_dir(profile.path()) {
            for entry in inner.flatten() {
                let _ = fs::remove_dir_all(entry.path().join("incremental"));
            }
        }
    }
}

/// Remove built trees other than `keep` (revisions still served).
pub(crate) fn prune_trees(layout: &Layout, keep: &[&str]) {
    let Ok(entries) = fs::read_dir(layout.trees()) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !keep.contains(&name.as_str()) {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

/// Remove prepared-map directories of other fingerprints than `keep`.
pub(crate) fn prune_presets(layout: &Layout, keep: &[&str]) {
    let Ok(entries) = fs::read_dir(layout.all_presets()) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !keep.contains(&name.as_str()) {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

/// Bytes below `path` (a footprint report, not a quota).
pub(crate) fn size(path: &Path) -> u64 {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return 0;
    };
    if meta.is_dir() {
        fs::read_dir(path)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| size(&e.path()))
            .sum()
    } else {
        meta.len()
    }
}
