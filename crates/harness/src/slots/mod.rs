//! Heavy slots: a machine-wide counting semaphore for the work that compiles or runs the
//! workspace's tests (docs/gates.md). At most N heavy gates run at once on one machine,
//! across every worktree of the repository; the rest queue first come, first served.
//!
//! It is the ship lock's ticket queue (`queue`) with capacity N: a ticket is alive while
//! it waits and while it runs, and may run when fewer than N older tickets are alive. The
//! kernel frees the slot of a dead holder; there are no PID files and no process scans.
//! A holder passes its slot to the processes it starts in `AOE_HEAVY_SLOT`
//! (`<ticket>:<nonce>`), which a child accepts only when the ticket exists, is alive and
//! carries that nonce, so setting the variable by hand does not skip the queue.
//! The deadline of a gate counts from when it starts running, never from the wait.
pub(crate) mod guard;
mod load;
pub(crate) mod queue;
mod status;

#[cfg(test)]
mod tests;

use crate::ship::lock::utc;
use queue::{Result, Ticket, read_json};
use serde::{Deserialize, Serialize};
use std::{
    ffi::OsString,
    path::Path,
    thread,
    time::{Duration, Instant},
};

pub(crate) use guard::{cap_args, hold, hold_snapshot, run_forced};

/// `run_forced` for the checkout the process runs in.
pub(crate) fn run_here(gate: &str, argv: &[String], slots: &SlotEnv) -> queue::Result<i32> {
    run_forced(Path::new("."), gate, argv, slots)
}
pub(crate) use status::print_status;

/// The environment variable that carries a held slot to child processes.
pub(crate) const TOKEN_VAR: &str = "AOE_HEAVY_SLOT";
/// The shared slot directory a holder names for its children (a snapshot's own Git directory is not it).
pub(crate) const DIR_VAR: &str = "AOE_HEAVY_DIR";
/// The CPUs each container started under the held slot may use: set by the holder next to
/// `AOE_HEAVY_SLOT`, so it exists exactly where a slot does. The Makefile and the harness's own
/// `docker run` lines read it; unset means no slot and no cap.
pub(crate) const CPUS_VAR: &str = "AOE_SLOT_CPUS";
/// Environment variables forwarded into Docker by the Makefile.
pub(crate) const SLOTS_VAR: &str = "AOE_HEAVY_SLOTS";
pub(crate) const WAIT_VAR: &str = "AOE_HEAVY_WAIT_MINUTES";
const DEFAULT_WAIT_MINUTES: u64 = 90;
const MAX_SLOTS: usize = 8;
pub(crate) const POLL: Duration = Duration::from_secs(1);
pub(crate) const REPORT_EVERY: Duration = Duration::from_secs(300);

/// The `AOE_HEAVY_*` variables as values. Only `SlotEnv::process` reads the real environment,
/// once, at the process entry point; everything below it is given a `SlotEnv`, so no test depends
/// on the environment of the process that runs it (the audit in `slots/tests/environment.rs`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SlotEnv {
    /// `AOE_HEAVY_SLOT`: the token of the slot a parent holds.
    pub(crate) token: Option<String>,
    /// `AOE_HEAVY_DIR`: the shared queue the parent named.
    pub(crate) dir: Option<OsString>,
    /// `AOE_HEAVY_SLOTS`: the slot count override.
    pub(crate) slots: Option<String>,
    /// `AOE_HEAVY_WAIT_MINUTES`: the wait override.
    pub(crate) wait: Option<String>,
}

impl SlotEnv {
    pub(crate) fn process() -> Self {
        Self {
            token: std::env::var(TOKEN_VAR).ok(),
            dir: std::env::var_os(DIR_VAR),
            slots: std::env::var(SLOTS_VAR).ok(),
            wait: std::env::var(WAIT_VAR).ok(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Holder {
    pub(crate) worktree: String,
    pub(crate) branch: String,
    pub(crate) command: String,
    pub(crate) started: u64,
    /// Secret proof of ownership; only the holder and its children carry it.
    pub(crate) nonce: String,
}

impl Holder {
    fn describe(&self) -> String {
        format!(
            "`{}` on {} in {} (started {})",
            self.command,
            self.branch,
            self.worktree,
            utc(self.started)
        )
    }

    fn place(&self) -> String {
        format!("{} in {}", self.command, self.worktree)
    }
}

/// A slot held until dropped, or one inherited from a parent that holds it.
#[derive(Debug)]
pub(crate) enum Slot {
    Owned {
        _ticket: Ticket<Holder>,
        token: String,
    },
    Inherited {
        token: String,
    },
}

impl Slot {
    pub(crate) fn token(&self) -> &str {
        match self {
            Self::Owned { token, .. } | Self::Inherited { token } => token,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Committed {
    slots: usize,
}

/// How many slots there are, how many cores the machine has, and how long to queue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Config {
    pub(crate) slots: usize,
    pub(crate) cores: usize,
    pub(crate) wait: Duration,
}

impl Config {
    /// `gates/heavy.json` sets the default slots; the cores are the machine's CPU affinity list
    /// (`process::load::cores`), the same count the load check uses. `AOE_HEAVY_SLOTS` (1-8) and
    /// `AOE_HEAVY_WAIT_MINUTES` (default 90; 0 fails at once) override.
    pub(crate) fn load(root: &Path, slots: &SlotEnv) -> Result<Self> {
        let text = std::fs::read_to_string(root.join("gates/heavy.json"))?;
        let committed: Committed = serde_json::from_str(&text)?;
        Self::from_values(
            committed.slots,
            crate::process::load::cores(),
            slots.slots.as_deref(),
            slots.wait.as_deref(),
        )
    }

    /// The CPUs each slot's containers may use: the cores shared out evenly, at least one.
    pub(crate) fn cpus_per_slot(&self) -> usize {
        (self.cores / self.slots).max(1)
    }

    pub(crate) fn from_values(
        default_slots: usize,
        cores: usize,
        slots: Option<&str>,
        wait: Option<&str>,
    ) -> Result<Self> {
        let slots = match slots.filter(|text| !text.is_empty()) {
            Some(text) => text.trim().parse::<usize>().map_err(|_| {
                format!("{SLOTS_VAR}={text}: use a whole number from 1 to {MAX_SLOTS}")
            })?,
            None => default_slots,
        };
        if !(1..=MAX_SLOTS).contains(&slots) {
            return Err(format!("heavy slots must be from 1 to {MAX_SLOTS}, not {slots}").into());
        }
        let minutes = match wait.filter(|text| !text.is_empty()) {
            Some(text) => text
                .trim()
                .parse::<u64>()
                .map_err(|_| format!("{WAIT_VAR}={text}: use a whole number of minutes"))?,
            None => DEFAULT_WAIT_MINUTES,
        };
        if cores == 0 {
            return Err("heavy slots: the machine reports no cores".into());
        }
        Ok(Self {
            slots,
            cores,
            wait: Duration::from_secs(minutes.saturating_mul(60)),
        })
    }
}

/// Whether `token` names a live ticket in `dir` that carries its nonce.
pub(crate) fn verify(dir: &Path, token: &str) -> bool {
    let Some((name, nonce)) = token.split_once(':') else {
        return false;
    };
    !nonce.is_empty()
        && queue::is_alive(dir, name)
        && read_json::<Holder>(&dir.join(name)).is_some_and(|holder| holder.nonce == nonce)
}

/// Take a slot (or accept the inherited one), queueing for at most `wait`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn acquire_at(
    dir: &Path,
    slots: usize,
    holder: &Holder,
    inherited: Option<&str>,
    wait: Duration,
    poll: Duration,
    report_every: Duration,
    say: &mut dyn FnMut(&str),
) -> Result<Slot> {
    if let Some(token) = inherited.filter(|token| verify(dir, token)) {
        return Ok(Slot::Inherited {
            token: token.to_owned(),
        });
    }
    let started = Instant::now();
    let ticket = Ticket::enter(dir, holder)?;
    let mut announced = false;
    let mut last_position = 0;
    let mut last_report = started;
    loop {
        let ahead = ticket.ahead()?;
        if ahead.len() < slots {
            if announced {
                say(&format!(
                    "heavy slot: free after {}s; starting",
                    started.elapsed().as_secs()
                ));
            }
            let token = format!("{}:{}", ticket.name(), holder.nonce);
            return Ok(Slot::Owned {
                _ticket: ticket,
                token,
            });
        }
        let position = ahead.len() - slots + 1;
        let blocker = ahead
            .iter()
            .flatten()
            .next()
            .map_or("another heavy gate".into(), Holder::place);
        if started.elapsed() >= wait {
            let holders: Vec<String> = ahead
                .iter()
                .take(slots)
                .map(|h| h.as_ref().map_or("unknown".into(), Holder::describe))
                .collect();
            return Err(format!(
                "heavy slot: no free slot after {} min ({WAIT_VAR}); position {position} of the \
                 queue behind {}; the slots are held by: {}. Run `make load-status`, then try \
                 again when one finishes",
                wait.as_secs() / 60,
                ahead.len(),
                holders.join("; ")
            )
            .into());
        }
        if !announced {
            say(&format!(
                "heavy slot: waiting for {blocker} (position {position})"
            ));
            announced = true;
            last_position = position;
        } else if position != last_position {
            say(&format!("heavy slot: position {position}"));
            last_position = position;
        } else if last_report.elapsed() >= report_every {
            say(&format!(
                "heavy slot: still waiting for {blocker} ({} min so far, position {position})",
                started.elapsed().as_secs() / 60
            ));
            last_report = Instant::now();
        }
        thread::sleep(poll.min(wait.saturating_sub(started.elapsed())));
    }
}
