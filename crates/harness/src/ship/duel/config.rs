//! Bounded task input and environment-only configuration for the main session.
use crate::{agents::Runtime, ship::git};
use std::{fs, io::Read, path::Path, time::Duration};

#[derive(Debug)]
pub(crate) struct Options {
    pub(crate) runtime: Runtime,
    pub(crate) model: Option<String>,
    pub(crate) effort: Option<String>,
    pub(crate) rounds: u32,
    pub(crate) check: Option<String>,
    pub(crate) budget: Duration,
    /// DUEL_QUALIFY: a command run once after the last green round (for example `make preflight`).
    pub(crate) qualify: Option<String>,
    pub(crate) qualify_budget: Duration,
    /// Implementer fixes DUEL_QUALIFY_ATTEMPTS allows before the duel ends FAIL.
    pub(crate) qualify_attempts: u32,
}

impl Options {
    pub(crate) fn from_env(get: impl Fn(&str) -> Option<String>) -> git::Result<Self> {
        let runtime = get("DUEL_RUNTIME").or_else(|| {
            let mut detected: Vec<_> = [("CLAUDECODE", "claude"), ("CODEX_THREAD_ID", "codex"),
                ("CODEX_CI", "codex"), ("PI_CODING_AGENT", "pi")]
                .into_iter().filter_map(|(key, runtime)| get(key).map(|_| runtime)).collect();
            detected.sort_unstable();
            detected.dedup();
            (detected.len() == 1).then(|| detected[0].to_owned())
        }).ok_or("DUEL_RUNTIME is required when the calling runtime is unknown or ambiguous (claude, codex or pi)")?;
        let runtime = clap::ValueEnum::from_str(&runtime, true)
            .map_err(|_| "DUEL_RUNTIME: use claude, codex or pi")?;
        if runtime == Runtime::Dsh {
            return Err("dsh is not supported yet for duel".into());
        }
        let number = |key, default| -> git::Result<u32> {
            get(key).map_or(Ok(default), |v| {
                v.parse()
                    .map_err(|_| format!("{key}: expected a positive integer"))
            })
        };
        let rounds = number("DUEL_ROUNDS", 2)?;
        let minutes = number("DUEL_BUDGET_MINUTES", 60)?;
        if !(1..=4).contains(&rounds) {
            return Err("DUEL_ROUNDS must be between 1 and 4".into());
        }
        if minutes == 0 {
            return Err("DUEL_BUDGET_MINUTES must be positive".into());
        }
        let qualify_minutes = number("DUEL_QUALIFY_MINUTES", 45)?;
        if qualify_minutes == 0 {
            return Err("DUEL_QUALIFY_MINUTES must be positive".into());
        }
        let qualify_attempts = number("DUEL_QUALIFY_ATTEMPTS", 2)?;
        if !(1..=5).contains(&qualify_attempts) {
            return Err("DUEL_QUALIFY_ATTEMPTS must be between 1 and 5".into());
        }
        Ok(Self {
            runtime,
            model: get("DUEL_MODEL"),
            effort: get("DUEL_EFFORT"),
            rounds,
            check: get("DUEL_CHECK"),
            budget: Duration::from_secs(u64::from(minutes) * 60),
            qualify: get("DUEL_QUALIFY"),
            qualify_budget: Duration::from_secs(u64::from(qualify_minutes) * 60),
            qualify_attempts,
        })
    }
}

pub(crate) fn read_task(path: &Path) -> git::Result<String> {
    let meta = fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !meta.is_file() {
        return Err("DUEL_TASK must be a regular file".into());
    }
    if meta.len() > 65536 {
        return Err("DUEL_TASK exceeds 64 KiB".into());
    }
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut text = String::new();
    file.take(65537)
        .read_to_string(&mut text)
        .map_err(|e| e.to_string())?;
    if text.len() > 65536 {
        return Err("DUEL_TASK exceeds 64 KiB".into());
    }
    if text.lines().next().is_none_or(|s| s.trim().is_empty()) {
        return Err("DUEL_TASK needs a nonempty first line".into());
    }
    Ok(text)
}
