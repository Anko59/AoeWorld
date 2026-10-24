//! Skirmish AI opponents for the development server: the server starts the
//! Python AI hosts itself (`AOE_AI_COMMAND`, docs/ai-sdk.md), so its
//! container is the SDK image (the tools image plus Python) and the
//! `aoeworld` wheel is installed into a site directory under `target/`.
use super::Result;
use std::path::{Path, PathBuf};

/// The tools image plus Python; it runs the release server unchanged.
pub(super) const SDK_IMAGE: &str = "aoeworld/sdk-python:1.15.0";
const SITE: &str = "target/dev-ai/site";

fn site(root: &Path) -> PathBuf {
    root.join(SITE)
}

/// Installs the newest built wheel into the site directory. Without a wheel
/// the server starts without AI opponents, and says why.
pub(super) fn install(root: &Path) -> Result<bool> {
    let Ok(wheel) = crate::sdk_python::newest_wheel(&root.join("target/wheels")) else {
        println!("AI opponents unavailable: no aoeworld wheel (`make dev` builds one)");
        return Ok(false);
    };
    let site = site(root);
    if site.exists() {
        std::fs::remove_dir_all(&site)?;
    }
    std::fs::create_dir_all(&site)?;
    let user = format!(
        "{}:{}",
        nix::unistd::Uid::current(),
        nix::unistd::Gid::current()
    );
    let mount = format!("{0}:{0}", root.display());
    let target = site.to_string_lossy().into_owned();
    let wheel = wheel.to_string_lossy().into_owned();
    super::docker(&[
        "run",
        "--rm",
        "--user",
        &user,
        "-e",
        "HOME=/tmp",
        "-v",
        &mount,
        SDK_IMAGE,
        "pip",
        "install",
        "--quiet",
        "--no-deps",
        "--no-cache-dir",
        "--target",
        &target,
        &wheel,
    ])?;
    Ok(true)
}

/// The server container's image and environment: with an installed site,
/// the SDK image and the AI host command; otherwise the plain tools image.
pub(super) fn container(root: &Path) -> (&'static str, Vec<String>) {
    let site = site(root);
    if !site.join("aoeworld").is_dir() {
        return (super::RUST_TOOLS_IMAGE, Vec::new());
    }
    let environment = [
        "AOE_AI_COMMAND=python3 -m aoe_ai host".to_owned(),
        format!(
            "PYTHONPATH={}:{}",
            site.display(),
            root.join("ai").display()
        ),
        "PYTHONDONTWRITEBYTECODE=1".to_owned(),
        "HOME=/tmp".to_owned(),
    ];
    let arguments = environment
        .into_iter()
        .flat_map(|value| ["-e".to_owned(), value])
        .collect();
    (SDK_IMAGE, arguments)
}

#[cfg(test)]
#[path = "tests/ai.rs"]
mod tests;
