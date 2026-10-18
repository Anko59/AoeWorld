//! The process-level view of the heavy slots: which gates take one, and holding it.
use super::{
    CPUS_VAR, Config, DIR_VAR, Holder, POLL, REPORT_EVERY, Slot, SlotEnv, TOKEN_VAR, acquire_at,
    queue::Result,
};
use crate::{
    gates::registry::{Cadence, Registry},
    ship::git,
};
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

/// What a process holding a slot hands to the processes it starts: the token and the directory.
type Env = Vec<(&'static str, String)>;

/// The environment of the slot this process holds (itself or inherited), so a nested heavy
/// gate in the same process (preflight running test-unit) passes it on instead of taking another.
static HELD: Mutex<Option<Env>> = Mutex::new(None);

/// Supervised children inherit admission even when the caller took its slot in
/// this process rather than receiving it through the process environment.
pub(crate) fn child_env() -> Env {
    HELD.lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .unwrap_or_default()
}

/// The `docker run` arguments that cap a container to the held slot's CPUs: `--cpus N` when this
/// process holds or inherited a slot, nothing otherwise. The containers the harness starts itself
/// (browsers, chromedriver) take it right after `run`.
pub(crate) fn cap_args() -> Vec<String> {
    cap_args_for(&child_env())
}

pub(super) fn cap_args_for(env: &Env) -> Vec<String> {
    env.iter()
        .find(|(name, _)| *name == CPUS_VAR)
        .map(|(_, cpus)| vec!["--cpus".to_owned(), cpus.clone()])
        .unwrap_or_default()
}

/// A slot (or nothing, for light or nested work) for as long as this lives.
#[derive(Debug)]
pub(crate) struct Guard {
    _slot: Option<Slot>,
    env: Env,
    outermost: bool,
}

impl Guard {
    fn none() -> Self {
        Self {
            _slot: None,
            env: Vec::new(),
            outermost: false,
        }
    }

    /// The environment that hands this slot to a child process.
    pub(crate) fn env(&self) -> Env {
        self.env.clone()
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        if self.outermost {
            *HELD.lock().unwrap_or_else(|e| e.into_inner()) = None;
        }
    }
}

/// `<git common dir>/aoe-ship/heavy-slots`, shared by every worktree of the repository.
pub(crate) fn dir(root: &Path) -> Result<PathBuf> {
    let common = git::git(
        root,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )
    .map_err(|error| {
        format!(
            "heavy slot: no git directory for {} ({error}); cannot find the shared slot queue",
            root.display()
        )
    })?;
    Ok(PathBuf::from(common).join("aoe-ship/heavy-slots"))
}

/// The shared slot directory: the one a holder named for its children, else the repository's.
pub(crate) fn slot_dir(root: &Path, slots: &SlotEnv) -> Result<PathBuf> {
    slot_dir_with(slots.dir.as_deref(), root)
}

pub(crate) fn slot_dir_with(inherited: Option<&OsStr>, root: &Path) -> Result<PathBuf> {
    match inherited.filter(|value| !value.is_empty()) {
        Some(value) => Ok(PathBuf::from(value)),
        None => dir(root),
    }
}

/// The checkout (or snapshot) holding `gates/registry.json` at or above `from`, so a gate run
/// from a subdirectory such as `fuzz/` reads the same registry as the root. `None` outside every
/// checkout: there is no registry, so no gate there is heavy.
pub(crate) fn checkout_of(from: &Path) -> Result<Option<PathBuf>> {
    let start = from
        .canonicalize()
        .map_err(|error| format!("heavy slot: cannot resolve {} ({error})", from.display()))?;
    Ok(start
        .ancestors()
        .find(|dir| dir.join("gates/registry.json").is_file())
        .map(Path::to_path_buf))
}

/// Whether the registry marks heavy the gate named `gate`: the one whose id is that name or whose
/// `command` is `make <name>` (a gate's id is its make target). `preflight` is heavy when any gate
/// of the preflight cadence is, since it runs them under one slot.
pub(crate) fn is_heavy(registry: &Registry, gate: &str) -> bool {
    if gate == "test-harness" {
        return true;
    }
    if gate == "preflight" {
        return registry
            .gates
            .iter()
            .any(|g| g.heavy && g.cadences.contains(&Cadence::Preflight));
    }
    let command = format!("make {gate}");
    registry
        .gates
        .iter()
        .any(|g| g.heavy && (g.id == gate || g.command == command))
}

/// The slot for a harness command, named by its subcommand: a make target runs its harness
/// command under the same name, so the registry decides for both ways of starting a gate.
pub(crate) fn guard(gate: &str, slots: &SlotEnv) -> Result<Guard> {
    hold(Path::new("."), gate, slots)
}

/// Take a slot for `gate` if the registry says it is heavy, announcing a wait on stderr. Returns
/// at once, holding nothing, for light gates; a process that already holds one passes it on.
pub(crate) fn hold(from: &Path, gate: &str, slots: &SlotEnv) -> Result<Guard> {
    match checkout_of(from)? {
        Some(root) => hold_in(&root, gate, slots, || slot_dir(&root, slots), load_wait),
        None => Ok(Guard::none()),
    }
}

/// Take a slot for `gate` whatever the registry says: the `slot` command runs make targets that
/// compile or test without being gates (`build`, `duel-check`), and each of them queues like a gate.
pub(crate) fn hold_forced(from: &Path, gate: &str, slots: &SlotEnv) -> Result<Guard> {
    let root = checkout_of(from)?.ok_or_else(|| {
        format!(
            "heavy slot: {gate} needs a checkout with gates/registry.json; {} is not one",
            from.display()
        )
    })?;
    acquire_in(&root, gate, slots, || slot_dir(&root, slots), load_wait)
}

fn load_wait() -> Result<()> {
    let cancellation = crate::process::Cancellation::default();
    #[cfg(target_os = "linux")]
    let _signals = crate::gates::runner::signals::Signals::new(&cancellation)?;
    super::load::wait(&cancellation)
}

/// A gate run in the snapshot checkout `root` of the real checkout `source`: it queues in
/// `source`'s shared directory, not in the snapshot's own Git directory (which is deleted with it).
pub(crate) fn hold_snapshot(
    root: &Path,
    source: &Path,
    gate: &str,
    env: &SlotEnv,
    cancellation: &crate::process::Cancellation,
) -> Result<Guard> {
    hold_in(
        root,
        gate,
        env,
        || slot_dir(source, env),
        || super::load::wait(cancellation),
    )
}

pub(super) fn hold_in(
    root: &Path,
    gate: &str,
    env: &SlotEnv,
    queue: impl FnOnce() -> Result<PathBuf>,
    wait_load: impl FnOnce() -> Result<()>,
) -> Result<Guard> {
    if !root.join("gates/registry.json").is_file() {
        return Ok(Guard::none());
    }
    let registry = Registry::load(root).map_err(|error| {
        format!(
            "heavy slot: cannot read the registry of {} ({error})",
            root.display()
        )
    })?;
    if !is_heavy(&registry, gate) {
        return Ok(Guard::none());
    }
    acquire_in(root, gate, env, queue, wait_load)
}

/// Hold the slot of this process (or pass on the one it holds) for `gate` in `root`.
pub(super) fn acquire_in(
    root: &Path,
    gate: &str,
    env: &SlotEnv,
    queue: impl FnOnce() -> Result<PathBuf>,
    wait_load: impl FnOnce() -> Result<()>,
) -> Result<Guard> {
    let mut held = HELD.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(env) = held.as_ref() {
        return Ok(Guard {
            _slot: None,
            env: env.clone(),
            outermost: false,
        });
    }
    let queue = queue()?;
    let config = Config::load(root, env)?;
    let holder = Holder {
        worktree: root
            .canonicalize()
            .unwrap_or_else(|_| root.to_owned())
            .display()
            .to_string(),
        branch: git::branch(root).unwrap_or_else(|_| "(detached)".into()),
        command: gate.into(),
        started: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs()),
        nonce: nonce(),
    };
    let slot = acquire_at(
        &queue,
        config.slots,
        &holder,
        env.token.as_deref(),
        config.wait,
        POLL,
        REPORT_EVERY,
        &mut |line| eprintln!("{line}"),
    )?;
    // An inherited slot is a child of a gate whose deadline has already started.
    // Its parent checked load before that deadline; never wait again inside it.
    let cap = cap_for(
        &config,
        crate::ship::run::in_ci(|name| std::env::var(name).ok()),
    );
    if matches!(slot, Slot::Owned { .. }) {
        match &cap {
            Some(cpus) => {
                eprintln!("heavy slot: {gate} holds a slot; its containers get {cpus} CPUs each")
            }
            None => eprintln!(
                "heavy slot: {gate} holds a slot; CI runs one job per runner, so no CPU cap"
            ),
        }
        wait_load()?;
    }
    let mut env = vec![
        (TOKEN_VAR, slot.token().to_owned()),
        (DIR_VAR, queue.display().to_string()),
    ];
    if let Some(cpus) = cap {
        env.push((CPUS_VAR, cpus));
    }
    *held = Some(env.clone());
    Ok(Guard {
        _slot: Some(slot),
        env,
        outermost: true,
    })
}

/// The CPU cap a slot exports: none in CI, where each job has a runner to itself and a
/// cap of `cores / slots` only slows timing-sensitive browser tests (#474's e2e runs).
pub(crate) fn cap_for(config: &Config, ci: bool) -> Option<String> {
    (!ci).then(|| config.cpus_per_slot().to_string())
}

/// Run `argv` as gate `gate` holding its slot, and return its exit code. The orchestrator
/// container runs a make target's steps this way, so the slot is taken before they start.
pub(crate) fn run_forced(from: &Path, gate: &str, argv: &[String], env: &SlotEnv) -> Result<i32> {
    run_with_guard(argv, hold_forced(from, gate, env)?)
}

pub(super) fn run_with_guard(argv: &[String], guard: Guard) -> Result<i32> {
    let (program, args) = argv
        .split_first()
        .ok_or("heavy slot: no command to run under the slot")?;
    let status = std::process::Command::new(program)
        .args(args)
        .envs(guard.env())
        .status()
        .map_err(|error| format!("heavy slot: cannot start {program} ({error})"))?;
    drop(guard);
    Ok(status.code().unwrap_or(1))
}

/// 128 random bits as hex (the kernel's randomness; the clock if it cannot be read).
pub(crate) fn nonce() -> String {
    use std::io::Read;
    let mut bytes = [0u8; 16];
    let read = std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut bytes));
    if read.is_err() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        bytes = blake3::hash(&nanos.to_le_bytes()).as_bytes()[..16]
            .try_into()
            .unwrap_or([0; 16]);
    }
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
