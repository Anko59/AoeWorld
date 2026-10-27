//! The human's view: a pinned Chromium container running `browser/scale/match-fps.mjs`.
use super::{BoxError, Options};
use std::{
    path::Path,
    process::{Child, Command, Stdio},
};

pub(super) const SCRIPT: &str = "scale/match-fps.mjs";

pub(super) fn container_name(options: &Options) -> String {
    format!("aoe-scale-browser-{}", options.port)
}

/// The `docker run` arguments of the browser container.
pub(super) fn docker_args(options: &Options, root: &Path, out: &Path) -> Vec<String> {
    let user = format!(
        "{}:{}",
        nix::unistd::Uid::current(),
        nix::unistd::Gid::current()
    );
    let mount = format!("{}:{}", root.display(), root.display());
    let modules = format!(
        "{0}/browser/node_modules:{0}/browser/node_modules:ro",
        root.display()
    );
    let seconds = (options.browser_minutes * 60.0).round() as u64;
    let mut args: Vec<String> = [
        "run",
        "--rm",
        "--init",
        "--network",
        "host",
        "--ipc",
        "host",
    ]
    .map(str::to_owned)
    .into();
    args.extend(crate::slots::cap_args());
    args.extend([
        "--user".to_owned(),
        user,
        "--name".to_owned(),
        container_name(options),
        "-e".to_owned(),
        format!("HOME={}/.cache/browser-home", root.display()),
        "-e".to_owned(),
        format!("SCALE_BASE_URL=http://127.0.0.1:{}", options.port),
        "-e".to_owned(),
        format!("SCALE_OUT={}", out.display()),
        "-e".to_owned(),
        format!("SCALE_SECONDS={seconds}"),
        "-e".to_owned(),
        format!("SCALE_RECORD_VIDEO={}", u8::from(options.record_video)),
        "-v".to_owned(),
        mount,
        "-v".to_owned(),
        modules,
        "-w".to_owned(),
        format!("{}/browser", root.display()),
        options.browser_image.clone(),
        "xvfb-run".to_owned(),
        "-a".to_owned(),
        "node".to_owned(),
        SCRIPT.to_owned(),
    ]);
    args
}

pub(super) fn spawn(options: &Options, root: &Path, out: &Path) -> Result<Child, BoxError> {
    std::fs::create_dir_all(root.join(".cache/browser-home"))?;
    let log = std::fs::File::create(out.with_extension("log"))?;
    Ok(Command::new("docker")
        .args(docker_args(options, root, out))
        .stdout(Stdio::from(log.try_clone()?))
        .stderr(Stdio::from(log))
        .spawn()?)
}
