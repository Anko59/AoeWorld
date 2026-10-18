//! The `docker run` command line of the pinned browser container.
use super::shard::Shard;
use std::path::Path;

pub(super) struct Run<'a> {
    pub root: &'a Path,
    /// `AOE_BASE_URLS`: one game server per Playwright worker.
    pub base_urls: String,
    pub container: String,
    pub landscape_hash: &'a str,
    pub specs: &'a [String],
    pub shard: Option<Shard>,
}

/// A shard names its container, so logs and `docker ps` show which part of the suite runs.
pub(super) fn container_name(process: u32, port: u16, shard: Option<Shard>) -> String {
    match shard {
        Some(shard) => format!(
            "aoeworld-e2e-shard-{}-of-{}-{process}-{port}",
            shard.index, shard.count
        ),
        None => format!("aoeworld-e2e-{process}-{port}"),
    }
}

pub(super) fn docker_args(run: &Run) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let root = run.root;
    let user = format!(
        "{}:{}",
        nix::unistd::Uid::current(),
        nix::unistd::Gid::current()
    );
    let mount = format!("{}:{}", root.display(), root.display());
    let modules = root
        .join("browser/node_modules")
        .canonicalize()
        .map_err(|e| format!("{}/browser/node_modules: {e}", root.display()))?;
    let dependencies = format!("{}:{}:ro", modules.display(), modules.display());
    let mut args: Vec<String> = [
        "run",
        "--rm",
        "--init",
        "--network",
        "host",
        "--ipc",
        "host",
        "--user",
        &user,
        "--name",
        &run.container,
    ]
    .map(str::to_owned)
    .into();
    args.splice(1..1, crate::slots::cap_args());
    // The gap at start lets `aoe-harness recover` find this container after a host suspend.
    let gap = crate::process::suspend::Watch::start(&crate::process::suspend::read)
        .gap_seconds()
        .map_or_else(String::new, |gap| gap.to_string());
    args.extend([
        "--label".to_owned(),
        format!("{}={gap}", crate::process::suspend::LABEL),
        "-e".to_owned(),
        format!("HOME={}/.cache/browser-home", root.display()),
        "-e".to_owned(),
        format!("AOE_BASE_URLS={}", run.base_urls),
        "-e".to_owned(),
        format!("AOE_E2E_LANDSCAPE_HASH={}", run.landscape_hash),
        "-v".to_owned(),
        mount,
        "-v".to_owned(),
        dependencies,
        "-w".to_owned(),
        root.join("browser").display().to_string(),
        "aoeworld/browser-tools:1.63.0".to_owned(),
        "xvfb-run".to_owned(),
        "-a".to_owned(),
        "npm".to_owned(),
    ]);
    let focused = !run.specs.is_empty();
    if focused {
        args.push("--silent".to_owned());
    }
    args.extend(["run".to_owned(), "test:e2e".to_owned()]);
    if focused {
        args.extend(["--".to_owned(), "--reporter=json".to_owned()]);
        args.extend(run.specs.iter().cloned());
    } else if let Some(shard) = run.shard {
        args.extend(["--".to_owned(), format!("--shard={shard}")]);
    }
    Ok(args)
}

#[cfg(test)]
mod tests;
