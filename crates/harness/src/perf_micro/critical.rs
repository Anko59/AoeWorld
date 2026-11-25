//! The critical-path instruction count of a kernel run on worker threads.
//!
//! Critical path = the main thread's instructions + for each parallel phase
//! the busiest worker's instructions in that phase. Total over every thread
//! counts the work, the main thread the serial part; neither tracks the wall
//! time a tick waits on once work moves to workers.
//!
//! A phase is one parallel call site: the jobs rayon starts from an idle
//! worker's loop (`WorkerThread::wait_until_cold` at recursion level 1, so a
//! job run while another waits is counted once, inside its parent), labelled
//! by the last `aoe_…` path in the job's type name (the closure's function or
//! the item type). Per label, the busiest worker over the whole window is
//! taken, so the figure is a lower bound of the sum of per-call maxima and at
//! least the total over the workers divided by their number. Work stealing
//! and the idle search loop are not phase work and are left out.
use crate::perf_scale::callgrind::{self, Profile};
use std::{collections::BTreeMap, fs, path::Path};

const IDLE_LOOP: &str = "<rayon_core::registry::WorkerThread>::wait_until_cold";
const JOB: &str = "<rayon_core::job::";
/// Callgrind's thread 1 is the thread that started the process: the serial path.
const MAIN_THREAD: u32 = 1;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Critical {
    pub main: u64,
    pub total: u64,
    pub workers: usize,
    /// Label to the busiest worker's instructions in that phase.
    pub phases: BTreeMap<String, u64>,
}

impl Critical {
    pub fn path(&self) -> u64 {
        self.main + self.phases.values().sum::<u64>()
    }

    pub fn note(&self) -> String {
        format!(
            "informational, not gated until a dev run sets its baseline: critical path {} = main thread {} + busiest worker per phase {} over {} phases ({} workers, total over all threads {})",
            self.path(),
            self.main,
            self.phases.values().sum::<u64>(),
            self.phases.len(),
            self.workers,
            self.total
        )
    }
}

/// The last `aoe_…` path of a job's type name, with the method of an
/// `<aoe_…::Type>::method` closure owner; `other` when it names none.
pub fn label(name: &str) -> String {
    let bytes = name.as_bytes();
    let word = |c: u8| c.is_ascii_alphanumeric() || c == b'_' || c == b':';
    let mut last = None;
    let mut at = 0;
    while let Some(found) = name[at..].find("aoe_") {
        let start = at + found;
        at = start + 4;
        if start > 0 && word(bytes[start - 1]) {
            continue;
        }
        let mut end = start;
        while end < bytes.len() && word(bytes[end]) {
            end += 1;
        }
        if name[end..].starts_with(">::") {
            end += 3;
            while end < bytes.len() && word(bytes[end]) {
                end += 1;
            }
        }
        last = Some(name[start..end].trim_end_matches(':').to_owned());
    }
    last.unwrap_or_else(|| "other".to_owned())
}

/// Instructions of the jobs a thread started from its idle loop, per label.
fn jobs(profile: &Profile, ir: usize) -> BTreeMap<String, u64> {
    let mut jobs = BTreeMap::new();
    for ((caller, callee), cost) in &profile.edges {
        if caller == IDLE_LOOP && callee.starts_with(JOB) {
            *jobs.entry(label(callee)).or_default() += cost[ir];
        }
    }
    jobs
}

/// Combines one profile per thread (`thread` as Callgrind numbers them).
pub fn critical(threads: &[(u32, Profile)]) -> Result<Critical, String> {
    let mut result = Critical::default();
    let mut main = false;
    for (thread, profile) in threads {
        let ir = profile.event("Ir")?;
        let total = *profile.totals.get(ir).ok_or("totals lack Ir")?;
        result.total += total;
        if *thread == MAIN_THREAD {
            result.main = total;
            main = true;
            continue;
        }
        result.workers += 1;
        for (label, cost) in jobs(profile, ir) {
            let busiest = result.phases.entry(label).or_default();
            *busiest = (*busiest).max(cost);
        }
    }
    if main {
        Ok(result)
    } else {
        Err("no profile of thread 1 (the main thread)".into())
    }
}

fn thread_of(text: &str) -> Option<u32> {
    text.lines()
        .take(12)
        .find_map(|line| line.strip_prefix("thread:"))
        .and_then(|number| number.trim().parse().ok())
}

/// A kernel's `callgrind.<name>.tN.pM.out` files of one directory.
pub fn read_dir(dir: &Path) -> Result<Critical, String> {
    let mut threads = Vec::new();
    let entries = fs::read_dir(dir).map_err(|error| format!("{}: {error}", dir.display()))?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.ends_with(".out") || !name.contains(".t") || !name.contains(".p") {
            continue;
        }
        let text = fs::read_to_string(entry.path()).map_err(|error| error.to_string())?;
        let thread = thread_of(&text).ok_or_else(|| format!("{name} names no thread"))?;
        threads.push((thread, callgrind::parse(&text)?));
    }
    if threads.len() < 2 {
        return Err(format!("{} holds no per-thread profiles", dir.display()));
    }
    critical(&threads)
}

/// The directory Gungraun keeps a benchmark's profiles in:
/// `target/gungraun/<package>/<benchmark module path>`.
pub fn locate(project_root: &Path, module_path: &str) -> Option<std::path::PathBuf> {
    let relative: std::path::PathBuf = module_path.split("::").collect();
    [project_root.join("target"), "target".into()]
        .iter()
        .filter_map(|target| fs::read_dir(target.join("gungraun")).ok())
        .flatten()
        .flatten()
        .map(|package| package.path().join(&relative))
        .find(|dir| dir.is_dir())
}

#[cfg(test)]
mod tests;
