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
    let inherited_slot = std::env::var("AOE_LONG_SLOT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let inherited = std::env::var_os("AOE_LONG_DIR")
        .map(PathBuf::from)
        .map(|dir| lock::slot_path(&dir, inherited_slot));
    let inherited = inherited.filter(|p| lock::verify(p, &token).unwrap_or(false));
    if focused || (!locked && inherited.is_some()) {
        return Err(command.exec().into());
    }
    let dir = PathBuf::from(git(&[
        "rev-parse",
        "--path-format=absolute",
        "--git-common-dir",
    ])?)
    .join("aoe-ship");
    if locked {
        let slot: usize = std::env::var("AOE_LONG_SLOT")?.parse()?;
        let path = lock::slot_path(&dir, slot);
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
        let me = super::holder(format!("`make {gate}`"))?;
        super::cpu::ledger(&dir, "long", "admitted", &me, &[("slot", slot.to_string())]);
        command
            .env("AOE_LONG_JOB", token)
            .env("AOE_LONG_DIR", &dir)
            .env("AOE_LONG_SLOT", slot.to_string());
        return Err(command.exec().into());
    }
    fs::create_dir_all(&dir)?;
    use std::os::unix::fs::OpenOptionsExt;
    let slots = lock::slot_count(std::env::var("AOE_LONG_SLOTS").ok().as_deref())?;
    let mut holders = Vec::new();
    for slot in 0..slots {
        let path = lock::slot_path(&dir, slot);
        fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .mode(0o600)
            .open(&path)?;
        // flock opens the same Linux lock as File::try_lock. --no-fork execs our
        // metadata writer, retaining the lock fd through Make and its children.
        // A conflict code no job is expected to return means "slot taken".
        let status = Command::new("flock")
            .args([
                "--nonblock",
                "--conflict-exit-code",
                SLOT_TAKEN,
                "--no-fork",
            ])
            .arg(&path)
            .arg(std::env::current_exe()?)
            .arg("--locked")
            .args(args)
            .env("AOE_LONG_SLOT", slot.to_string())
            .status()?;
        let code = status.code().unwrap_or(1);
        if code.to_string() != SLOT_TAKEN {
            return Ok(code);
        }
        let holder = lock::status(&path)?.unwrap_or_else(|| "holder just finished".into());
        holders.push(format!("slot {slot}: {holder}"));
    }
    eprintln!("long job busy: {}, retry later", holders.join("; "));
    let me = super::holder(format!("`make {gate}`"))?;
    super::cpu::ledger(
        &dir,
        "long",
        "refused",
        &me,
        &[("reason", holders.join("; "))],
    );
    Ok(75)
}

/// flock's conflict exit code; mapped to 75 only when every slot is held.
const SLOT_TAKEN: &str = "197";

fn git(args: &[&str]) -> Result<String> {
    let output = Command::new("git").args(args).output()?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned().into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().into())
}
