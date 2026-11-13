//! Machine-wide build pool: each short `docker run` holds one slot while it runs.
//! A slot is about `CARGO_BUILD_JOBS` (4) compilers, so the default pool of 4 keeps
//! the 12-core host near 16 compilers. One worktree, and one coordinator with all
//! its agents, may hold only part of the pool. A nested container inherits its
//! parent's slot or long-job slot instead of taking another.
use super::lock;
use std::{
    error::Error,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
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
/// A short job waits this long for a slot, then is refused; never an open queue.
const WAIT: Duration = Duration::from_secs(90);

/// Who holds a slot. Stored as tab-separated fields after the lock's proof line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Holder {
    pub(crate) worktree: String,
    pub(crate) coordinator: String,
    pub(crate) description: String,
}

impl Holder {
    fn encode(&self) -> String {
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

/// The admission decision for one job, given every slot's current holder.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Plan {
    /// Free slots to try, in order.
    Try(Vec<usize>),
    Refuse(String),
}

pub(crate) fn plan(held: &[Option<Holder>], me: &Holder) -> Plan {
    let mine = held
        .iter()
        .flatten()
        .filter(|h| h.worktree == me.worktree)
        .count();
    if mine >= PER_WORKTREE {
        return Plan::Refuse(format!(
            "{} already runs {mine} build containers (limit {PER_WORKTREE} per worktree)",
            me.worktree
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
                "{} and its agents already run {ours} build containers (limit {PER_COORDINATOR} per coordinator)",
                me.coordinator
            ));
        }
    }
    let free: Vec<usize> = (0..held.len())
        .filter(|&slot| held[slot].is_none())
        .collect();
    if free.is_empty() {
        let holders: Vec<String> = held
            .iter()
            .enumerate()
            .filter_map(|(slot, h)| {
                h.as_ref()
                    .map(|h| format!("build slot {slot}: {}", h.display()))
            })
            .collect();
        return Plan::Refuse(format!(
            "all {} build slots are held: {}",
            held.len(),
            holders.join("; ")
        ));
    }
    Plan::Try(free)
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

/// A held build slot; dropping it releases the kernel lock.
pub(crate) struct Slot {
    _lock: lock::LongLock,
    /// `<lock path>:<proof>`, passed to the container so nested jobs inherit it.
    pub(crate) token: String,
}

/// Admit one build container, waiting up to `WAIT` for a slot. `None` means the
/// caller already runs inside an admitted job and needs no slot of its own.
pub(crate) fn admit(dir: &Path, me: &Holder, wait: Option<Duration>) -> Result<Option<Slot>> {
    if inherited()? {
        return Ok(None);
    }
    let slots = slot_count(std::env::var(SLOTS_VAR).ok().as_deref())?;
    admit_with(dir, me, slots, wait.unwrap_or(WAIT)).map(Some)
}

/// Admission against `slots` build slots, ignoring any inherited parent slot.
pub(crate) fn admit_with(dir: &Path, me: &Holder, slots: usize, wait: Duration) -> Result<Slot> {
    let deadline = Instant::now() + wait;
    let mut announced = false;
    loop {
        let reason = match plan(&holders(dir, slots)?, me) {
            Plan::Try(free) => {
                let proof = nonce()?;
                let mut last = String::new();
                for slot in free {
                    let path = slot_path(dir, slot);
                    match lock::LongLock::acquire(&path, &me.encode(), &proof, None) {
                        Ok(held) => {
                            let token = format!("{}:{proof}", path.display());
                            return Ok(Slot { _lock: held, token });
                        }
                        Err(error) if error.is::<lock::Busy>() => last = error.to_string(),
                        Err(error) => return Err(error),
                    }
                }
                last
            }
            Plan::Refuse(reason) => reason,
        };
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

/// Inside a long job or an admitted container, the parent's slot covers this one.
fn inherited() -> Result<bool> {
    if let Ok(token) = std::env::var(TOKEN_VAR)
        && let Some((path, proof)) = token.rsplit_once(':')
        && lock::verify(Path::new(path), proof)?
    {
        return Ok(true);
    }
    let long = std::env::var("AOE_LONG_JOB").unwrap_or_default();
    let slot = std::env::var("AOE_LONG_SLOT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    match std::env::var_os("AOE_LONG_DIR") {
        Some(dir) => lock::verify(&lock::slot_path(Path::new(&dir), slot), &long),
        None => Ok(false),
    }
}

fn nonce() -> Result<String> {
    let mut bytes = [0u8; 16];
    std::io::Read::read_exact(&mut File::open("/dev/urandom")?, &mut bytes)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// One JSON line per admission, refusal or finish, for `make load-status` and tuning.
pub(crate) fn ledger(dir: &Path, event: &str, me: &Holder, extra: &[(&str, String)]) {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let mut line = format!(
        "{{\"ts\":{seconds},\"kind\":\"build\",\"event\":{},\"worktree\":{},\"coordinator\":{},\"command\":{}",
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
