//! The Python AI hosts: sibling `aoeworld/sdk-python` containers.
use super::{BoxError, Options};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

pub(super) struct Host {
    pub name: String,
    pub child: Child,
    pub report: PathBuf,
    pub log: PathBuf,
}

fn docker_python(options: &Options, root: &Path, name: &str, site: &Path) -> Command {
    let mut command = Command::new("docker");
    command
        .args(["run", "--rm", "--init"])
        .args(crate::slots::cap_args())
        .args(["--network", "host", "--user"])
        .arg(format!(
            "{}:{}",
            nix::unistd::Uid::current(),
            nix::unistd::Gid::current()
        ))
        .args([
            "--name",
            name,
            "-e",
            "HOME=/tmp",
            "-e",
            "PYTHONDONTWRITEBYTECODE=1",
        ])
        .arg("-e")
        .arg(format!(
            "PYTHONPATH={}:{}/ai",
            site.display(),
            root.display()
        ))
        .arg("-v")
        .arg(format!("{0}:{0}", root.display()))
        .arg("-w")
        .arg(root)
        .arg(&options.bot_image);
    command
}

fn install_wheel(options: &Options, root: &Path, site: &Path) -> Result<(), BoxError> {
    let wheel = crate::sdk_python::newest_wheel(&root.join("target/wheels"))
        .map_err(|e| format!("{e}: run `make sdk-python` first"))?;
    let _ = fs::remove_dir_all(site);
    let status = docker_python(options, root, "aoe-scale-wheel", site)
        .args([
            "python3",
            "-m",
            "pip",
            "install",
            "--no-deps",
            "--no-cache-dir",
            "--target",
        ])
        .arg(site)
        .arg(&wheel)
        .stdout(Stdio::null())
        .status()?;
    if !status.success() {
        return Err("installing the aoeworld wheel failed".into());
    }
    Ok(())
}

/// Splits `tokens` among `hosts` processes and starts each in its container.
pub(super) fn start_hosts(
    options: &Options,
    root: &Path,
    work: &Path,
    tokens: &[String],
) -> Result<Vec<Host>, BoxError> {
    let site = work.join("site");
    install_wheel(options, root, &site)?;
    let per = tokens.len().div_ceil(options.hosts.max(1)).max(1);
    let mut hosts = Vec::new();
    for (index, group) in tokens.chunks(per).enumerate() {
        let tokens_file = work.join(format!("tokens-{index}.json"));
        fs::write(&tokens_file, serde_json::to_vec(group)?)?;
        let report = work.join(format!("bots-{index}.json"));
        let _ = fs::remove_file(&report);
        let name = format!("aoe-scale-bots-{}-{index}", options.port);
        let log_path = work.join(format!("bots-{index}.log"));
        let log = fs::File::create(&log_path)?;
        let child = docker_python(options, root, &name, &site)
            .args(["python3", "-m", "aoe_ai", "host", "--url"])
            .arg(format!("http://127.0.0.1:{}", options.port))
            .arg("--tokens-file")
            .arg(&tokens_file)
            .arg("--max-seconds")
            .arg((options.minutes * 60.0).round().to_string())
            .arg("--report")
            .arg(&report)
            .stdout(Stdio::from(log.try_clone()?))
            .stderr(Stdio::from(log))
            .spawn()?;
        hosts.push(Host {
            name,
            child,
            report,
            log: log_path,
        });
    }
    Ok(hosts)
}

/// Waits until every host has joined all of its slots (each prints
/// `N bots connected in X s`); returns the seconds that took.
pub(super) fn wait_connected(hosts: &mut [Host]) -> Result<f64, BoxError> {
    let started = std::time::Instant::now();
    loop {
        let mut pending = 0;
        for host in hosts.iter_mut() {
            let text = fs::read_to_string(&host.log).unwrap_or_default();
            if !text.contains("bots connected in") {
                if let Some(status) = host.child.try_wait()? {
                    return Err(format!(
                        "{} exited before its bots connected: {status}",
                        host.name
                    )
                    .into());
                }
                pending += 1;
            }
        }
        if pending == 0 {
            return Ok(started.elapsed().as_secs_f64());
        }
        if started.elapsed() > std::time::Duration::from_secs(15 * 60) {
            return Err(format!("{pending} bot hosts had not connected after 15 minutes").into());
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
}
