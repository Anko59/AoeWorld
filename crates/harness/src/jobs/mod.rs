//! Short work starts immediately; long jobs share N kernel-lock slots.
pub(crate) mod guard;
pub(crate) mod lock;
pub(crate) use guard::{cap_args, hold, hold_snapshot, run_forced};
use std::{ffi::OsString, path::Path};
#[cfg(test)]
mod tests;

pub(crate) type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub(crate) const TOKEN_VAR: &str = "AOE_LONG_JOB";
pub(crate) const DIR_VAR: &str = "AOE_LONG_DIR";
pub(crate) const SLOT_VAR: &str = "AOE_LONG_SLOT";
pub(crate) const SLOTS_VAR: &str = "AOE_LONG_SLOTS";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct JobEnv {
    pub(crate) token: Option<String>,
    pub(crate) dir: Option<OsString>,
    /// The slot the owning job holds; absent means slot 0.
    pub(crate) slot: Option<usize>,
    /// Raw `AOE_LONG_SLOTS`; absent means the default count.
    pub(crate) slots: Option<String>,
    pub(crate) ci: bool,
    pub(crate) e2e_specs: Option<String>,
}

impl JobEnv {
    pub(crate) fn process() -> Self {
        Self {
            token: std::env::var(TOKEN_VAR).ok(),
            dir: std::env::var_os(DIR_VAR),
            slot: std::env::var(SLOT_VAR).ok().and_then(|s| s.parse().ok()),
            slots: std::env::var(SLOTS_VAR).ok(),
            ci: crate::ship::run::in_ci(|name| std::env::var(name).ok()),
            e2e_specs: std::env::var("AOE_E2E_SPECS").ok(),
        }
    }
}

pub(crate) fn run_here(gate: &str, argv: &[String], env: &JobEnv) -> Result<i32> {
    run_forced(Path::new("."), gate, argv, env)
}

pub(crate) fn print_status(root: &Path, env: &JobEnv) -> Result<()> {
    let available =
        crate::resources::policy::available_kib(&std::fs::read_to_string("/proc/meminfo")?)?;
    println!(
        "MemAvailable: {available} KiB (admission floor: {} KiB)",
        crate::resources::policy::FLOOR_KIB
    );
    let dir = guard::lock_dir(root, env)?;
    let slots = lock::slot_count(env.slots.as_deref())?;
    println!(
        "long job slots: {slots} (AOE_LONG_SLOTS, 1..={})",
        lock::MAX_SLOTS
    );
    for slot in 0..slots {
        match lock::status(&lock::slot_path(&dir, slot))? {
            Some(holder) => println!("long job slot {slot}: {holder}"),
            None => println!("long job slot {slot}: free"),
        }
    }
    Ok(())
}
