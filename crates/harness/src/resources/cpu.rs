//! Machine-wide build pool: each `docker run` holds one slot while it runs.
//! A slot is about `CARGO_BUILD_JOBS` (4) compilers, so the default pool of 4 keeps
//! the 12-core host near 16 compilers. Running long jobs count against the pool, one
//! worktree may hold only part of it, and so may one coordinator with all its agents.
//! A nested container uses its parent's build or long-job slot instead of another.
//!
//! Admission runs under one short machine-wide lock: read the holders, pick a slot,
//! and wait until the new holder has it. `flock --no-fork` takes the slot and execs
//! the launcher's `--build-locked` mode, which records the holder and execs Docker,
//! so the kernel lock lives exactly as long as the Docker client.
use super::lock;
use std::{
    error::Error,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::{Child, Command},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

pub(crate) const TOKEN_VAR: &str = "AOE_CPU_TOKEN";
pub(crate) const SLOTS_VAR: &str = "AOE_CPU_SLOTS";
pub(crate) const COORDINATOR_VAR: &str = "AOE_COORDINATOR";
pub(crate) const DEFAULT_SLOTS: usize = 4;
pub(crate) const MAX_SLOTS: usize = 16;
pub(crate) const PER_WORKTREE: usize = 2;
pub(crate) const PER_COORDINATOR: usize = 3;
/// A build waits this long for a slot, then is refused; never an open queue.
pub(crate) const WAIT: Duration = Duration::from_secs(90);
/// flock's exit code when another process took the slot first.
const TAKEN: &str = "197";

/// Who holds a slot. Stored as tab-separated fields after the lock's proof line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Holder {
    pub(crate) worktree: String,
    pub(crate) coordinator: String,
    pub(crate) description: String,
}

impl Holder {
    pub(crate) fn encode(&self) -> String {
        format!(
            "{}\t{}\t{}",
            self.worktree, self.coordinator, self.description
        )
    }

    pub(crate) fn decode(text: &str) -> Self {
        let mut fields = text.splitn(3, '\t');
        let mut next = || fields.next().unwrap_or_default().to_owned();
        let (worktree, coordinator, description) = (next(), next(), next());
        Self {
            worktree,
            coordinator,
            description,
        }
    }

    pub(crate) fn display(&self) -> String {
        let by = if self.coordinator.is_empty() {
            String::new()
        } else {
            format!(" for {}", self.coordinator)
        };
        format!("{} in {}{by}", self.description, self.worktree)
    }
}

/// The admission decision for one build, given every build and long-job holder.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Plan {
    /// Free build slots to try, in order.
    Try(Vec<usize>),
    Refuse(String),
}

pub(crate) fn plan(held: &[Option<Holder>], long: &[String], me: &Holder) -> Plan {
    let listed = |keep: &dyn Fn(&Holder) -> bool| -> String {
        held.iter()
            .enumerate()
            .filter_map(|(slot, h)| {
                h.as_ref()
                    .filter(|h| keep(h))
                    .map(|h| format!("build slot {slot}: {}", h.display()))
            })
            .collect::<Vec<_>>()
            .join("; ")
    };
    let mine = held
        .iter()
        .flatten()
        .filter(|h| h.worktree == me.worktree)
        .count();
    if mine >= PER_WORKTREE {
        return Plan::Refuse(format!(
            "{} already runs {mine} build containers (limit {PER_WORKTREE} per worktree): {}",
            me.worktree,
            listed(&|h| h.worktree == me.worktree)
        ));
    }
    if !me.coordinator.is_empty() {
        let ours = held
            .iter()
            .flatten()
            .filter(|h| h.coordinator == me.coordinator)
            .count();
        if ours >= PER_COORDINATOR {
            return Plan::Refuse(format!(
                "{} and its agents already run {ours} build containers (limit {PER_COORDINATOR} per coordinator): {}",
                me.coordinator,
                listed(&|h| h.coordinator == me.coordinator)
            ));
        }
    }
    let busy = held.iter().flatten().count() + long.len();
    if busy >= held.len() {
        let mut holders = vec![listed(&|_| true)];
        holders.extend(long.iter().map(|h| format!("long job: {h}")));
        holders.retain(|h| !h.is_empty());
        return Plan::Refuse(format!(
            "{busy} of {} build slots are in use, counting long jobs: {}",
            held.len(),
            holders.join("; ")
        ));
    }
    Plan::Try(
        (0..held.len())
            .filter(|&slot| held[slot].is_none())
            .collect(),
    )
}

pub(crate) fn slot_count(raw: Option<&str>) -> Result<usize> {
    match raw.map(str::trim).filter(|text| !text.is_empty()) {
        None => Ok(DEFAULT_SLOTS),
        Some(text) => text
            .parse::<usize>()
            .ok()
            .filter(|count| (1..=MAX_SLOTS).contains(count))
            .ok_or_else(|| format!("{SLOTS_VAR} must be 1..={MAX_SLOTS}, got {text:?}").into()),
    }
}

pub(crate) fn slot_path(dir: &Path, slot: usize) -> PathBuf {
    dir.join(format!("cpu-{slot}.lock"))
}

pub(crate) fn holders(dir: &Path, slots: usize) -> Result<Vec<Option<Holder>>> {
    (0..slots)
        .map(|slot| Ok(lock::status(&slot_path(dir, slot))?.map(|text| Holder::decode(&text))))
        .collect()
}

fn long_holders(dir: &Path, slots: usize) -> Result<Vec<String>> {
    (0..slots)
        .filter_map(|slot| lock::status(&lock::slot_path(dir, slot)).transpose())
        .collect()
}

/// What a build inside another admitted job inherits.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Parent {
    None,
    /// An admitted build container: pass its token on to this container too.
    Build(String),
    /// A long job, whose variables Make already passes into containers.
    Long,
}

pub(crate) fn parent() -> Result<Parent> {
    if let Ok(token) = std::env::var(TOKEN_VAR)
        && let Some((path, proof)) = token.rsplit_once(':')
        && lock::verify(Path::new(path), proof)?
    {
        return Ok(Parent::Build(token));
    }
    let long = std::env::var("AOE_LONG_JOB").unwrap_or_default();
    let slot = std::env::var("AOE_LONG_SLOT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let held = match std::env::var_os("AOE_LONG_DIR") {
        Some(dir) => lock::verify(&lock::slot_path(Path::new(&dir), slot), &long)?,
        None => false,
    };
    Ok(if held { Parent::Long } else { Parent::None })
}

/// `docker run …` with `-e AOE_CPU_TOKEN=<token>` right after `run`.
pub(crate) fn with_token(args: &[String], token: &str) -> Vec<String> {
    let mut args = args.to_vec();
    args.splice(1..1, ["-e".to_owned(), format!("{TOKEN_VAR}={token}")]);
    args
}

/// Start `program args` holding a build slot, waiting up to `wait` for one.
/// `launcher` is the executable whose `--build-locked` mode records the holder.
pub(crate) fn start(
    dir: &Path,
    me: &Holder,
    slots: (usize, usize),
    wait: Duration,
    launcher: &Path,
    program: &str,
    args: &[String],
) -> Result<(Child, usize)> {
    fs::create_dir_all(dir)?;
    let deadline = Instant::now() + wait;
    let mut announced = false;
    loop {
        let gate = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(dir.join("cpu-admit.lock"))?;
        gate.lock()?;
        let reason = match plan(&holders(dir, slots.0)?, &long_holders(dir, slots.1)?, me) {
            Plan::Try(free) => {
                for slot in free {
                    if let Some(child) = launch(dir, slot, me, launcher, program, args)? {
                        drop(gate);
                        return Ok((child, slot));
                    }
                }
                "every free build slot was taken while admitting".to_owned()
            }
            Plan::Refuse(reason) => reason,
        };
        drop(gate);
        if Instant::now() >= deadline {
            return Err(Box::new(lock::Busy(format!(
                "{reason}; do non-build work and retry later (`make load-status` shows the holders)"
            ))));
        }
        if !announced {
            eprintln!("build pool: {reason}; waiting up to {} s", wait.as_secs());
            announced = true;
        }
        std::thread::sleep(Duration::from_secs(2));
    }
}

/// Take `slot` through flock and return once the new holder has recorded itself,
/// or `None` when another process holds the slot.
fn launch(
    dir: &Path,
    slot: usize,
    me: &Holder,
    launcher: &Path,
    program: &str,
    args: &[String],
) -> Result<Option<Child>> {
    let path = slot_path(dir, slot);
    OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)?;
    let proof = nonce()?;
    let token = format!("{}:{proof}", path.display());
    let mut child = Command::new("flock")
        .args(["--nonblock", "--conflict-exit-code", TAKEN, "--no-fork"])
        .arg(&path)
        .arg(launcher)
        .arg("--build-locked")
        .arg(&path)
        .arg(&proof)
        .arg(me.encode())
        .arg("--")
        .arg(program)
        .args(if program == "docker" {
            with_token(args, &token)
        } else {
            args.to_vec()
        })
        .spawn()?;
    // Only the process holding the slot writes this proof; polling with a lock
    // probe instead could briefly take the free slot from the starting flock.
    let recorded = format!("{proof}\n");
    loop {
        if fs::read_to_string(&path)?.starts_with(&recorded) {
            return Ok(Some(child));
        }
        if let Some(status) = child.try_wait()? {
            // A command that finished before we saw it still ran under the slot.
            return Ok(
                (status.code().map(|code| code.to_string()) != Some(TAKEN.into())).then_some(child),
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// The `--build-locked` mode, run by flock while it holds the slot: record the
/// holder, then become the command.
pub(crate) fn locked(args: &[String]) -> Result<std::convert::Infallible> {
    let [path, proof, holder, separator, program, rest @ ..] = args else {
        return Err("expected <slot> <proof> <holder> -- <command>".into());
    };
    if separator != "--" {
        return Err("expected -- before the command".into());
    }
    fs::write(path, format!("{proof}\n{holder}"))?;
    use std::os::unix::process::CommandExt;
    Err(Command::new(program).args(rest).exec().into())
}

fn nonce() -> Result<String> {
    let mut bytes = [0u8; 16];
    std::io::Read::read_exact(&mut File::open("/dev/urandom")?, &mut bytes)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// One JSON line per admission, refusal or finish, for `make load-status` and tuning.
pub(crate) fn ledger(dir: &Path, kind: &str, event: &str, me: &Holder, extra: &[(&str, String)]) {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let mut line = format!(
        "{{\"ts\":{seconds},\"kind\":{},\"event\":{},\"worktree\":{},\"coordinator\":{},\"command\":{}",
        quote(kind),
        quote(event),
        quote(&me.worktree),
        quote(&me.coordinator),
        quote(&me.description)
    );
    for (key, value) in extra {
        line.push_str(&format!(",{}:{}", quote(key), quote(value)));
    }
    line.push_str("}\n");
    // The ledger is diagnostic: a write failure never blocks a job.
    let _ = fs::create_dir_all(dir).and_then(|()| {
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("ledger.jsonl"))?
            .write_all(line.as_bytes())
    });
}

pub(crate) fn quote(text: &str) -> String {
    let mut out = String::from("\"");
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
