//! Standalone compiler adapter, built into the tool image without Cargo dependencies.
//! Only rustc runs under the stable source alias; orchestration keeps host paths.
use std::{
    collections::hash_map::DefaultHasher,
    env,
    hash::{Hash, Hasher},
    os::unix::{net::UnixStream, process::CommandExt},
    process::{Command, ExitCode},
};

const SOCKET: &str = "/aoe-sccache/server.sock";

fn normalize(value: &str, root: &str) -> String {
    if root.is_empty() || root == "/" {
        return value.to_owned();
    }
    let mut output = String::new();
    let mut end = 0;
    for (start, _) in value.match_indices(root) {
        output.push_str(&value[end..start]);
        let after = start + root.len();
        let left = start == 0 || matches!(value.as_bytes()[start - 1], b'=' | b':');
        let right = after == value.len() || matches!(value.as_bytes()[after], b'/' | b'=' | b':');
        output.push_str(if left && right { "/aoe-source" } else { root });
        end = after;
    }
    output.push_str(&value[end..]);
    output
}

fn service_name(directory: &str, identity: &str) -> String {
    let mut hash = DefaultHasher::new();
    directory.hash(&mut hash);
    format!(
        "aoeworld-sccache-0.19.0-{}-{:x}",
        identity.replace(':', "-"),
        hash.finish()
    )
}

fn service_arguments(directory: &str, identity: &str) -> Vec<String> {
    [
        "run",
        "--detach",
        "--name",
        &service_name(directory, identity),
        "--restart=unless-stopped",
        "--network",
        "none",
        "--cpus=1",
        "--memory=512m",
        "--user",
        identity,
        "--mount",
        &format!(
            "type=bind,\"source={}\",target=/aoe-sccache",
            directory.replace('"', "\"\"")
        ),
        "-e",
        "SCCACHE_DIR=/aoe-sccache/cache",
        "-e",
        "SCCACHE_CACHE_SIZE=5368709120",
        "-e",
        "SCCACHE_SERVER_UDS=/aoe-sccache/server.sock",
        "-e",
        "SCCACHE_IDLE_TIMEOUT=0",
        "-e",
        "SCCACHE_NO_DAEMON=1",
        "-e",
        "SCCACHE_START_SERVER=1",
        "aoeworld/rust-tools:1.99.1-sccache0.19.0",
        "sccache",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

fn service(action: &str, directory: &str, identity: &str) -> std::io::Result<bool> {
    let name = service_name(directory, identity);
    let start = || Command::new("docker").args(["start", &name]).output();
    Ok(match action {
        "start" => {
            // A racing worktree may create the named service between start and run.
            start()?.status.success()
                || Command::new("docker")
                    .args(service_arguments(directory, identity))
                    .status()?
                    .success()
                || start()?.status.success()
        }
        "stop" => Command::new("docker")
            .args(["stop", &name])
            .status()?
            .success(),
        "stats" => Command::new("docker")
            .args([
                "exec",
                "-e",
                "SCCACHE_START_SERVER=0",
                &name,
                "sccache",
                "--show-stats",
            ])
            .status()?
            .success(),
        _ => return Err(std::io::Error::other("expected start, stop or stats")),
    })
}

fn main() -> ExitCode {
    let arguments: Vec<_> = env::args_os().skip(1).collect();
    if arguments.first().is_some_and(|arg| arg == "--service") {
        let args: Vec<_> = arguments.iter().filter_map(|arg| arg.to_str()).collect();
        return match args.as_slice() {
            ["--service", action, directory, identity] => {
                match service(action, directory, identity) {
                    Ok(true) => ExitCode::SUCCESS,
                    result => {
                        eprintln!("compile cache service failed: {result:?}");
                        ExitCode::FAILURE
                    }
                }
            }
            _ => ExitCode::FAILURE,
        };
    }
    let Some(compiler) = arguments.first() else {
        return ExitCode::FAILURE;
    };
    let root = env::var("AOE_COMPILE_ROOT").ok();
    let mut command = if let Some(root) = root.filter(|_| UnixStream::connect(SOCKET).is_ok()) {
        let mut command = Command::new("sccache");
        command.arg(compiler);
        for arg in &arguments[1..] {
            command.arg(
                arg.to_str()
                    .map(|s| normalize(s, &root))
                    .map(Into::into)
                    .unwrap_or_else(|| arg.clone()),
            );
        }
        for (key, value) in env::vars_os() {
            if let Some(value) = value.to_str() {
                command.env(key, normalize(value, &root));
            }
        }
        if let Ok(cwd) = env::current_dir() {
            command.current_dir(normalize(&cwd.to_string_lossy(), &root));
        }
        command
    } else {
        let mut command = Command::new(compiler);
        command.args(&arguments[1..]);
        command
    };
    eprintln!("compile cache: cannot execute compiler: {}", command.exec());
    ExitCode::FAILURE
}

#[cfg(test)]
mod tests;
