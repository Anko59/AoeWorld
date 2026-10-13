//! The process-level view of the heavy slots: which commands take one, and holding it.
use super::{Config, Holder, POLL, REPORT_EVERY, Slot, TOKEN_VAR, acquire_at, queue::Result};
use crate::{
    Command,
    gates::registry::{Cadence, Registry},
    ship::git,
};
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

/// The token of the slot this process holds (itself or inherited), so a nested heavy
/// command in the same process (preflight running test-unit) does not take another.
static HELD: Mutex<Option<String>> = Mutex::new(None);

/// A slot (or nothing, for light or nested work) for as long as this lives.
#[derive(Debug)]
pub(crate) struct Guard {
    slot: Option<Slot>,
    outermost: bool,
}

impl Guard {
    fn none() -> Self {
        Self {
            slot: None,
            outermost: false,
        }
    }

    /// The environment that hands this slot to a child process.
    pub(crate) fn env(&self) -> Vec<(&'static str, String)> {
        self.slot
            .iter()
            .map(|slot| (TOKEN_VAR, slot.token().to_owned()))
            .collect()
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        if self.outermost {
            *HELD.lock().unwrap_or_else(|e| e.into_inner()) = None;
        }
    }
}

/// `<git common dir>/aoe-ship/heavy-slots`, shared by every worktree.
pub(crate) fn dir(root: &Path) -> Result<PathBuf> {
    let common = git::git(
        root,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    Ok(PathBuf::from(common).join("aoe-ship/heavy-slots"))
}

/// The gate a harness command runs, when it may be heavy (the registry decides).
fn gate_of(command: &Command) -> Option<&'static str> {
    Some(match command {
        Command::Lint => "lint",
        Command::TestUnit => "test-unit",
        Command::TestWasm => "test-wasm",
        Command::TestE2e => "test-e2e",
        Command::TestCreatorSource => "test-creator-source",
        Command::TestGeographicMatrix => "test-geographic-matrix",
        Command::TestGeographicVisuals => "test-geographic-visuals",
        Command::FuzzSmoke => "fuzz-smoke",
        Command::FuzzNightly => "fuzz-nightly",
        Command::MutationNightly(_) => "mutation-nightly",
        Command::PerfSmoke => "perf-smoke",
        Command::PerfCi => "perf-ci",
        Command::PerfFull => "perf-full",
        Command::PerfPressure => "perf-pressure",
        Command::PerfStress => "perf-stress",
        Command::PerfTimingReport => "perf-timing",
        Command::PerfSoak10 => "perf-soak-10",
        Command::PerfSoak30 => "perf-soak-30",
        Command::Preflight => "preflight",
        _ => return None,
    })
}

/// Whether the registry marks `gate` heavy. `preflight` is heavy when any gate of the
/// preflight cadence is, since it runs them under one slot.
pub(crate) fn is_heavy(registry: &Registry, gate: &str) -> bool {
    if gate == "preflight" {
        return registry
            .gates
            .iter()
            .any(|g| g.heavy && g.cadences.contains(&Cadence::Preflight));
    }
    registry.gates.iter().any(|g| g.id == gate && g.heavy)
}

/// The slot for a harness command, taken before it starts its deadline clock.
pub(crate) fn guard(command: &Command) -> Result<Guard> {
    match gate_of(command) {
        Some(gate) => hold(Path::new("."), gate),
        None => Ok(Guard::none()),
    }
}

/// Take a slot for `gate` if the registry says it is heavy, announcing a wait on stderr.
/// Returns at once, holding nothing, for light gates and when this process already holds one.
pub(crate) fn hold(root: &Path, gate: &str) -> Result<Guard> {
    let registry = match Registry::load(root) {
        Ok(registry) => registry,
        Err(error) => {
            eprintln!("heavy slot: no registry ({error}); running {gate} without a slot");
            return Ok(Guard::none());
        }
    };
    if !is_heavy(&registry, gate) {
        return Ok(Guard::none());
    }
    let mut held = HELD.lock().unwrap_or_else(|e| e.into_inner());
    if held.is_some() {
        return Ok(Guard::none());
    }
    let config = Config::load(root)?;
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
    let inherited = std::env::var(TOKEN_VAR).ok();
    let slot = acquire_at(
        &dir(root)?,
        config.slots,
        &holder,
        inherited.as_deref(),
        config.wait,
        POLL,
        REPORT_EVERY,
        &mut |line| eprintln!("{line}"),
    )?;
    *held = Some(slot.token().to_owned());
    Ok(Guard {
        slot: Some(slot),
        outermost: true,
    })
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
