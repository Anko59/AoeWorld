//! Preserve execution context while replacing only Cargo's operation with a fetch.
use super::*;

pub(super) fn copy(command: &Command) -> Command {
    let mut copy = context(command);
    copy.args(command.get_args());
    copy
}

fn context(command: &Command) -> Command {
    let mut copy = Command::new(command.get_program());
    if let Some(root) = command.get_current_dir() {
        copy.current_dir(root);
    }
    for (key, value) in command.get_envs() {
        match value {
            Some(value) => copy.env(key, value),
            None => copy.env_remove(key),
        };
    }
    copy
}

pub(super) fn fetch(command: &Command) -> Option<Command> {
    let args: Vec<_> = command.get_args().collect();
    let mut fetch = context(command);
    let cargo_args = match Path::new(command.get_program()).file_name()?.to_str()? {
        "cargo" => &args[..],
        "docker" => {
            let (image, prefix) = docker_prefix(&args)?;
            if Path::new(args.get(image + 1)?).file_name()? != "cargo" {
                return None;
            }
            // Retain the image, mounts, cwd and environment. Only the fetch is online.
            fetch.args(prefix).args(["-e", "CARGO_NET_OFFLINE=false"]);
            fetch.args(&args[image..image + 2]);
            &args[image + 2..]
        }
        _ => return None,
    };
    let mut args = cargo_args.iter().copied().peekable();
    if let Some(toolchain) = args
        .peek()
        .filter(|arg| arg.as_encoded_bytes().starts_with(b"+"))
    {
        fetch.arg(toolchain);
        args.next();
    }
    fetch.args(["fetch", "--locked"]);
    while let Some(arg) = args.next() {
        if matches!(
            arg.to_str(),
            Some("--manifest-path" | "--config" | "--target")
        ) {
            fetch.arg(arg);
            if let Some(value) = args.next() {
                fetch.arg(value);
            }
        } else if ["--manifest-path=", "--config=", "--target="]
            .iter()
            .any(|prefix| arg.as_encoded_bytes().starts_with(prefix.as_bytes()))
        {
            fetch.arg(arg);
        } else if arg == "--" {
            break;
        }
    }
    fetch.env("CARGO_NET_OFFLINE", "false");
    fetch.args(["--config", "net.offline=false"]);
    Some(fetch)
}

/// Parse the literal `docker run [options] IMAGE COMMAND` used by supervised
/// builds. Never mistake a volume, env value or shell string containing Cargo
/// for the executable. Unknown options fail closed rather than guessing arity.
fn docker_prefix<'a>(args: &[&'a OsStr]) -> Option<(usize, Vec<&'a OsStr>)> {
    if args.first()? != &"run" {
        return None;
    }
    let mut prefix = vec![args[0]];
    let mut index = 1;
    while let Some(arg) = args.get(index) {
        let text = arg.to_str()?;
        if text == "--" {
            return Some((index + 1, prefix));
        }
        if !text.starts_with('-') {
            return Some((index, prefix));
        }
        let option = text.split('=').next()?;
        let takes_value = match option {
            "-e" | "--env" | "-v" | "--volume" | "--mount" | "-w" | "--workdir" | "-u"
            | "--user" | "--network" | "--net" | "--name" | "--label" | "--cpus"
            | "--group-add" | "--tmpfs" | "--memory" | "--memory-swap" | "--ipc"
            | "--entrypoint" | "--env-file" | "--platform" | "--pull" | "--security-opt"
            | "--cap-add" | "--cap-drop" | "--device" | "--ulimit" | "--hostname"
            | "--add-host" => true,
            "--rm" | "--init" | "-i" | "--interactive" | "-t" | "--tty" | "--read-only" => false,
            _ => return None,
        };
        let end = index
            + if takes_value && !text.contains('=') {
                2
            } else {
                1
            };
        let option_args = args.get(index..end)?;
        let offline_network = matches!(option, "--network" | "--net")
            && (text.ends_with("=none") || option_args.get(1) == Some(&OsStr::new("none")));
        if !offline_network {
            prefix.extend_from_slice(option_args);
        }
        index = end;
    }
    None
}
