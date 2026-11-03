//! Short work starts immediately; only long jobs share an exclusive kernel lock.
pub(crate) mod guard;
pub(crate) mod lock;
pub(crate) use guard::{cap_args, hold, hold_snapshot, run_forced};
use std::{ffi::OsString, path::Path};
#[cfg(test)]
mod tests;

pub(crate) type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub(crate) const TOKEN_VAR: &str = "AOE_LONG_JOB";
pub(crate) const DIR_VAR: &str = "AOE_LONG_DIR";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct JobEnv {
    pub(crate) token: Option<String>,
    pub(crate) dir: Option<OsString>,
    pub(crate) ci: bool,
    pub(crate) e2e_specs: Option<String>,
}

impl JobEnv {
    pub(crate) fn process() -> Self {
        Self {
            token: std::env::var(TOKEN_VAR).ok(),
            dir: std::env::var_os(DIR_VAR),
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
    match lock::status(&guard::lock_path(root, env)?)? {
        Some(holder) => println!("long job: {holder}"),
        None => println!("long job: free"),
    }
    Ok(())
}
