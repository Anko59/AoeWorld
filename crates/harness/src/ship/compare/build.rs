//! The two builds a comparison may serve: the working tree (`after`) and the
//! merge base exported from Git (`before`). Both use the commands of `make dev`
//! (the wasm client, then the release server) in the pinned tool image; nothing
//! is built on the host and nothing is written outside `.cache/compare`, the
//! tree itself and its `target/`.
use crate::{dev::RUST_TOOLS_IMAGE, process};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// One cargo build of the whole app, cold, under load.
const BUILD_DEADLINE: Duration = Duration::from_secs(45 * 60);
/// Exported base trees kept besides the one in use.
const KEEP_BASES: usize = 1;

/// A tree that can serve the app: its sources, and the server executable.
pub(crate) struct Tree {
    pub(crate) root: PathBuf,
    pub(crate) binary: PathBuf,
}

fn user() -> String {
    format!("{}:{}", nix::unistd::getuid(), nix::unistd::getgid())
}

pub(crate) fn docker_tools(
    tree: &Path,
    target: &Path,
    cargo_home: &Path,
    args: &[&str],
) -> Result<()> {
    let mut all: Vec<String> = ["run", "--rm", "--init", "--user"]
        .map(String::from)
        .to_vec();
    all.push(user());
    for (name, value) in [
        ("CARGO_HOME", cargo_home.display().to_string()),
        ("CARGO_TARGET_DIR", target.display().to_string()),
    ] {
        all.extend(["-e".into(), format!("{name}={value}")]);
    }
    let mut mounts = vec![
        tree.to_path_buf(),
        cargo_home.to_path_buf(),
        target.to_path_buf(),
    ];
    mounts.sort();
    mounts.dedup();
    // A mount inside another is redundant (and the cargo cache lives in the checkout).
    let outer: Vec<PathBuf> = mounts
        .iter()
        .filter(|m| !mounts.iter().any(|o| o != *m && m.starts_with(o)))
        .cloned()
        .collect();
    for mount in outer {
        all.extend(["-v".into(), format!("{0}:{0}", mount.display())]);
    }
    all.extend([
        "-w".into(),
        tree.display().to_string(),
        RUST_TOOLS_IMAGE.into(),
    ]);
    all.extend(args.iter().map(|a| (*a).to_string()));
    let refs: Vec<&str> = all.iter().map(String::as_str).collect();
    process::run("docker", &refs, BUILD_DEADLINE)?;
    Ok(())
}

/// The wasm client into `web/pkg` and the release server into `target/release`
/// of `target`, as `make build-wasm` and `make dev` do.
pub(crate) fn build_app(tree: &Path, target: &Path, cargo_home: &Path) -> Result<()> {
    fs::create_dir_all(target)?;
    let wasm = target.join("wasm32-unknown-unknown/wasm-release/aoe_client.wasm");
    let wasm = wasm.display().to_string();
    for args in [
        &[
            "cargo",
            "build",
            "--locked",
            "--profile",
            "wasm-release",
            "--target",
            "wasm32-unknown-unknown",
            "-p",
            "aoe-client",
        ][..],
        &[
            "wasm-bindgen",
            "--target",
            "web",
            "--out-dir",
            "web/pkg",
            "--out-name",
            "aoe_client",
            &wasm,
        ][..],
        &[
            "cargo",
            "build",
            "--locked",
            "--release",
            "-p",
            "aoe-server",
        ][..],
    ] {
        eprintln!("compare: {} (in {})", args[..2].join(" "), tree.display());
        docker_tools(tree, target, cargo_home, args)?;
    }
    Ok(())
}

/// The working tree as it is now, built into its own `target/`.
pub(crate) fn working_tree(root: &Path) -> Result<Tree> {
    let target = root.join("target");
    build_app(root, &target, &root.join(".cache/cargo"))?;
    Ok(Tree {
        root: root.to_path_buf(),
        binary: target.join("release/aoe-server"),
    })
}

fn git(root: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git").args(args).current_dir(root).output()?;
    if !out.status.success() {
        return Err(format!(
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )
        .into());
    }
    Ok(String::from_utf8(out.stdout)?.trim().to_owned())
}

/// The commit the comparison's `before` is built from: COMPARE_BASE, else the
/// merge base of HEAD and origin/dev.
pub(crate) fn base_revision(root: &Path, requested: Option<&str>) -> Result<String> {
    let spec = match requested {
        Some(rev) => rev.to_owned(),
        None => git(root, &["merge-base", "HEAD", "origin/dev"])?,
    };
    let sha = git(
        root,
        &["rev-parse", "--verify", &format!("{spec}^{{commit}}")],
    )?;
    if sha.len() == 40 && sha.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(sha)
    } else {
        Err(format!("COMPARE_BASE `{spec}` is not a commit").into())
    }
}

/// `git archive <rev>` unpacked below `.cache/compare/base/<rev>` (no `.git`,
/// no worktree metadata, nothing changed in the repository), then built with
/// a target directory shared by every base so dependencies compile once.
pub(crate) fn base_tree(root: &Path, rev: &str) -> Result<Tree> {
    let bases = root.join(".cache/compare/base");
    let tree = bases.join(rev);
    let binary = tree.join("aoe-server.built");
    let done = tree.join("web/pkg/aoe_client_bg.wasm");
    if !(binary.is_file() && done.is_file()) {
        fs::create_dir_all(&bases)?;
        // A half-unpacked or half-built tree is thrown away, never trusted.
        let _ = fs::remove_dir_all(&tree);
        fs::create_dir_all(&tree)?;
        let mut archive = Command::new("git")
            .args(["archive", "--format=tar", rev])
            .current_dir(root)
            .stdout(Stdio::piped())
            .spawn()?;
        let tar = Command::new("tar")
            .args(["-x", "-f", "-", "-C"])
            .arg(&tree)
            .stdin(archive.stdout.take().ok_or("git archive has no output")?)
            .status()?;
        if !archive.wait()?.success() || !tar.success() {
            return Err(format!("could not export {rev} to {}", tree.display()).into());
        }
        let target = root.join(".cache/compare/target");
        build_app(&tree, &target, &root.join(".cache/cargo"))?;
        fs::copy(target.join("release/aoe-server"), &binary)?;
        prune(&bases, rev);
    }
    Ok(Tree { root: tree, binary })
}

/// Old exported bases are removed; the newest `KEEP_BASES` besides `keep` stay.
fn prune(bases: &Path, keep: &str) {
    let Ok(entries) = fs::read_dir(bases) else {
        return;
    };
    let mut others: Vec<_> = entries
        .flatten()
        .filter(|e| e.file_name() != keep)
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    others.sort();
    let excess = others.len().saturating_sub(KEEP_BASES);
    for (_, path) in others.into_iter().take(excess) {
        let _ = fs::remove_dir_all(path);
    }
}
