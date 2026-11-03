//! Admit before Cargo exists. flock keeps its descriptor across the entire exec tree.
use super::lock;
use std::{
    error::Error, fs, io::Read, os::unix::process::CommandExt, path::PathBuf, process::Command,
};
type Result<T> = std::result::Result<T, Box<dyn Error>>;

pub(super) fn run(args: &[String], locked: bool) -> Result<i32> {
    let (gate, rest) = args.split_first().ok_or("missing long target")?;
    let argv = rest
        .strip_prefix(&["--".into()])
        .ok_or("missing -- command")?;
    let (program, arguments) = argv.split_first().ok_or("missing command")?;
    let mut command = Command::new(program);
    command.args(arguments);
    let focused =
        gate == "test-e2e" && std::env::var("AOE_E2E_SPECS").is_ok_and(|s| !s.trim().is_empty());
    let token = std::env::var("AOE_LONG_JOB").unwrap_or_default();
    let inherited = std::env::var_os("AOE_LONG_DIR")
        .map(PathBuf::from)
        .map(|p| p.join("long.lock"));
    let inherited = inherited.filter(|p| lock::verify(p, &token).unwrap_or(false));
    if focused || (!locked && inherited.is_some()) {
        return Err(command.exec().into());
    }
    let path = PathBuf::from(git(&[
        "rev-parse",
        "--path-format=absolute",
        "--git-common-dir",
    ])?)
    .join("aoe-ship/long.lock");
    if locked {
        let mut bytes = [0u8; 16];
        fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
        let token: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        let date = Command::new("date")
            .args(["-u", "+%Y-%m-%d %H:%M:%SZ"])
            .output()?;
        if !date.status.success() {
            return Err("cannot determine UTC start".into());
        }
        let holder = format!(
            "`make {gate}` on {} in {} (started {})",
            git(&["branch", "--show-current"])?,
            std::env::current_dir()?.display(),
            String::from_utf8(date.stdout)?.trim()
        );
        fs::write(&path, format!("{token}\n{holder}"))?;
        eprintln!("long job admitted: {holder}");
        command
            .env("AOE_LONG_JOB", token)
            .env("AOE_LONG_DIR", path.parent().ok_or("missing lock parent")?);
        return Err(command.exec().into());
    }
    fs::create_dir_all(path.parent().ok_or("missing lock parent")?)?;
    use std::os::unix::fs::OpenOptionsExt;
    fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .mode(0o600)
        .open(&path)?;
    // flock opens the same Linux lock as File::try_lock. --no-fork execs our
    // metadata writer, retaining the lock fd through Make and its children.
    let status = Command::new("flock")
        .args(["--nonblock", "--conflict-exit-code", "75", "--no-fork"])
        .arg(&path)
        .arg(std::env::current_exe()?)
        .arg("--locked")
        .args(args)
        .status()?;
    let code = status.code().unwrap_or(1);
    if code == 75 {
        let holder = lock::status(&path)?.unwrap_or_else(|| "holder just finished".into());
        eprintln!("long job busy: {holder}, retry later");
    }
    Ok(code)
}

fn git(args: &[&str]) -> Result<String> {
    let output = Command::new("git").args(args).output()?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned().into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().into())
}
