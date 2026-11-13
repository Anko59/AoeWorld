//! Small host launcher, compiled independently of the workspace dependency graph.
#[allow(dead_code)]
mod cpu;
#[allow(dead_code)]
mod lock;
mod long;
mod observer;
mod policy;

#[cfg(test)]
mod tests;

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args
        .first()
        .is_some_and(|arg| arg == "--long" || arg == "--locked")
    {
        let locked = args.remove(0) == "--locked";
        std::process::exit(long::run(&args, locked).unwrap_or_else(|error| {
            eprintln!("long job: {error}");
            lock::exit_code(error.as_ref())
        }));
    }
    if args.first().is_some_and(|arg| arg == "--run") {
        args.remove(0);
        std::process::exit(run(&args).unwrap_or_else(|error| {
            eprintln!("resource admission: {error}");
            76
        }));
    }
    if args.first().is_some_and(|arg| arg == "--container") {
        args.remove(0);
        std::process::exit(observer::container(&args).unwrap_or_else(|error| {
            eprintln!("resource observer: {error}");
            1
        }));
    }
    eprintln!("resource runner: expected --run or --container followed by a command");
    std::process::exit(1);
}

fn run(args: &[String]) -> std::io::Result<i32> {
    let (program, args) = args
        .split_first()
        .ok_or_else(|| std::io::Error::other("resource run: missing command"))?;
    policy::check()?;
    let mut args = args.to_vec();
    let build = program == "docker" && args.first().is_some_and(|arg| arg == "run");
    let admitted = match build.then(|| admit_build(&args)).transpose() {
        Ok(admitted) => admitted.flatten(),
        Err(error) if error.is::<lock::Busy>() => {
            eprintln!("build pool {error}");
            return Ok(75);
        }
        Err(error) => return Err(std::io::Error::other(error.to_string())),
    };
    if let Some((_, _, slot)) = &admitted {
        args.splice(
            1..1,
            [
                "-e".to_owned(),
                format!("{}={}", cpu::TOKEN_VAR, slot.token),
            ],
        );
    }
    let started = std::time::Instant::now();
    let status = std::process::Command::new(program).args(&args).status()?;
    let code = status.code().unwrap_or(137);
    if let Some((dir, me, _slot)) = &admitted {
        let seconds = started.elapsed().as_secs().to_string();
        cpu::ledger(
            dir,
            "finished",
            me,
            &[("exit", code.to_string()), ("seconds", seconds)],
        );
    }
    if code == 137 {
        eprintln!(
            "resource: command killed (exit 137); container caps: {}; a kernel OOM kill means the memory cap was exceeded",
            policy::cap_args().join(" ")
        );
    }
    Ok(code)
}

type Admitted = (std::path::PathBuf, cpu::Holder, cpu::Slot);

/// Take a build slot for one `docker run`; `None` when a parent job's slot covers it.
fn admit_build(args: &[String]) -> Result<Option<Admitted>, Box<dyn std::error::Error>> {
    let dir = match std::env::var_os("AOE_LONG_DIR") {
        Some(dir) => std::path::PathBuf::from(dir),
        None => std::path::PathBuf::from(git(&[
            "rev-parse",
            "--path-format=absolute",
            "--git-common-dir",
        ])?)
        .join("aoe-ship"),
    };
    let worktree = git(&["rev-parse", "--show-toplevel"])
        .or_else(|_| std::env::current_dir().map(|dir| dir.display().to_string()))?;
    let me = cpu::Holder {
        worktree,
        coordinator: std::env::var(cpu::COORDINATOR_VAR).unwrap_or_default(),
        description: describe(args),
    };
    match cpu::admit(&dir, &me, None) {
        Ok(Some(slot)) => {
            cpu::ledger(&dir, "admitted", &me, &[]);
            Ok(Some((dir, me, slot)))
        }
        Ok(None) => Ok(None),
        Err(error) => {
            cpu::ledger(&dir, "refused", &me, &[("reason", error.to_string())]);
            Err(error)
        }
    }
}

/// The command a container runs: the words after its image, which is the first
/// argument naming an `aoeworld/` image.
fn describe(args: &[String]) -> String {
    let start = args
        .iter()
        .position(|arg| arg.starts_with("aoeworld/"))
        .map_or(0, |i| i + 1);
    let words: Vec<&str> = args[start..]
        .iter()
        .map(String::as_str)
        .filter(|word| !word.contains("resource-runner") && *word != "--container")
        .collect();
    let text = format!("`{}`", words.join(" "));
    match text.char_indices().nth(120) {
        Some((cut, _)) => format!("{}…`", &text[..cut]),
        None => text,
    }
}

fn git(args: &[&str]) -> std::io::Result<String> {
    let output = std::process::Command::new("git").args(args).output()?;
    if !output.status.success() {
        return Err(std::io::Error::other(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}
