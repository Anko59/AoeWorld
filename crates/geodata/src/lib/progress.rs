//! Optional process-local progress; these counters never determine publication.
use serde::Serialize;
use std::{
    cell::RefCell,
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    PreparingSources,
    DownloadingSource,
    SamplingOverview,
    SamplingWater,
    BuildingPyramids,
    PublishingPackage,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    Bytes,
    Pages,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
struct Progress {
    version: u8,
    phase: Phase,
    detail: Option<&'static str>,
    completed: Option<u64>,
    total: Option<u64>,
    unit: Option<Unit>,
}
struct Reporter {
    path: PathBuf,
    previous: Option<Progress>,
    written: Instant,
}
thread_local! { static REPORTER: RefCell<Option<Reporter>> = const { RefCell::new(None) }; }

/// Keep overview telemetry protected after the owning server dies too.
pub fn worker_lease(path: Option<&std::path::Path>) -> Result<Option<fs::File>, String> {
    let Some(path) = path else {
        return Ok(None);
    };
    let lease = path
        .parent()
        .ok_or("progress path has no scratch parent")?
        .join("lease");
    if !fs::symlink_metadata(&lease)
        .map_err(|error| error.to_string())?
        .is_file()
    {
        return Err("progress scratch lease is not a regular file".into());
    }
    let file = fs::File::open(&lease).map_err(|error| error.to_string())?;
    file.try_lock_shared()
        .map_err(|error| format!("progress scratch lease unavailable: {error}"))?;
    if !lease.is_file() {
        return Err("progress scratch was removed".into());
    }
    Ok(Some(file))
}

/// A worker request gets one isolated reporter; nested library calls share it.
pub struct Scope(Option<Reporter>);
impl Scope {
    pub fn new(path: Option<PathBuf>) -> Self {
        let next = path.map(|path| Reporter {
            path,
            previous: None,
            written: Instant::now(),
        });
        Self(REPORTER.with(|slot| slot.replace(next)))
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        REPORTER.with(|slot| slot.replace(self.0.take()));
    }
}
pub fn stage(phase: Phase) {
    report(phase, None, None);
}
pub fn count(phase: Phase, detail: Option<&'static str>, completed: u64, total: u64, unit: Unit) {
    if completed <= total && total > 0 {
        report(phase, detail, Some((completed, total, unit)));
    }
}
static TRACE_ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
static TRACE_START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
/// Anchors the trace clock at process start so the first phase line includes
/// the time spent before it (request preflight and source verification).
pub fn trace_start() {
    TRACE_START.get_or_init(Instant::now);
}
/// Same line for a named step inside a phase.
pub fn trace_mark(label: &str) {
    if !*TRACE_ON.get_or_init(|| std::env::var_os("AOE_PREP_TRACE").is_some()) {
        return;
    }
    trace_line(label);
}
/// With `AOE_PREP_TRACE` set, print one stderr line per phase change: wall and
/// process CPU seconds since the first report. CPU/wall near 1 means one thread.
fn trace(phase: Phase) {
    static LAST: std::sync::Mutex<Option<Phase>> = std::sync::Mutex::new(None);
    if !*TRACE_ON.get_or_init(|| std::env::var_os("AOE_PREP_TRACE").is_some()) {
        return;
    }
    let Ok(mut last) = LAST.lock() else {
        return;
    };
    if *last == Some(phase) {
        return;
    }
    *last = Some(phase);
    drop(last);
    trace_line(&format!("{phase:?}"));
}
fn trace_line(label: &str) {
    let start = TRACE_START.get_or_init(Instant::now);
    let cpu = fs::read_to_string("/proc/self/stat")
        .ok()
        .and_then(|text| {
            let fields = text
                .rsplit_once(')')?
                .1
                .split_whitespace()
                .collect::<Vec<_>>();
            let ticks =
                fields.get(11)?.parse::<f64>().ok()? + fields.get(12)?.parse::<f64>().ok()?;
            Some(ticks / 100.0)
        })
        .unwrap_or(f64::NAN);
    eprintln!(
        "prep-trace {label} wall={:.1}s cpu={cpu:.1}s",
        start.elapsed().as_secs_f64()
    );
}
fn report(phase: Phase, detail: Option<&'static str>, count: Option<(u64, u64, Unit)>) {
    trace(phase);
    let progress = Progress {
        version: 1,
        phase,
        detail,
        completed: count.map(|v| v.0),
        total: count.map(|v| v.1),
        unit: count.map(|v| v.2),
    };
    REPORTER.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(reporter) = slot.as_mut() else {
            return;
        };
        let same_phase = reporter
            .previous
            .is_some_and(|old| old.phase == phase && old.detail == detail);
        let final_count = progress.completed.is_some() && progress.completed == progress.total;
        if same_phase && !final_count && reporter.written.elapsed() < Duration::from_millis(250) {
            return;
        }
        let Ok(bytes) = serde_json::to_vec(&progress) else {
            return;
        };
        let temporary = reporter.path.with_extension("tmp");
        if fs::write(&temporary, bytes)
            .and_then(|()| fs::rename(&temporary, &reporter.path))
            .is_ok()
        {
            reporter.previous = Some(progress);
            reporter.written = Instant::now();
        }
    });
}
/// Overview fields sampled so far, out of all pyramid pages.
pub(crate) fn overview_pages(completed: u64, total: u64) {
    count(Phase::SamplingOverview, None, completed, total, Unit::Pages);
}
pub(crate) fn pyramid_page_count(mut axis: u16) -> u64 {
    let mut total = 0;
    while axis > 0 {
        let count = u64::from(axis.div_ceil(64));
        total += count * count;
        if axis == 1 {
            break;
        }
        axis = axis.div_ceil(2);
    }
    total
}
/// Sends a finished overview field's page count to the reporting thread.
pub(crate) fn landed<T>(
    sent: &std::sync::mpsc::Sender<u64>,
    result: Result<T, crate::GeodataError>,
    pages: impl Fn(&T) -> usize,
) -> Result<T, crate::GeodataError> {
    let _ = sent.send(result.as_ref().map_or(0, |field| pages(field) as u64));
    result
}

/// A sampling thread's result; a panic becomes a preparation error.
pub(crate) fn joined<T>(
    handle: std::thread::ScopedJoinHandle<'_, Result<T, crate::GeodataError>>,
) -> Result<T, crate::GeodataError> {
    let panicked = crate::GeodataError::Preparation("overview sampling thread panicked");
    handle.join().unwrap_or(Err(panicked))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn worker_progress_holds_a_shared_lease_until_execution_finishes() {
        let root = std::env::temp_dir().join(format!("aoe-progress-lease-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("progress.json");
        let lease_path = root.join("lease");
        assert!(worker_lease(Some(&path)).is_err());
        fs::write(&lease_path, []).unwrap();
        let worker = worker_lease(Some(&path)).unwrap();
        let janitor = fs::File::options()
            .read(true)
            .write(true)
            .open(&lease_path)
            .unwrap();
        assert!(janitor.try_lock().is_err());
        drop(worker);
        janitor.try_lock().unwrap();
        assert!(worker_lease(Some(&path)).is_err());
        drop(janitor);
        fs::remove_dir_all(root).unwrap();
        assert!(worker_lease(None).unwrap().is_none());
    }

    #[test]
    fn progress_is_scoped_atomic_and_reports_exact_final_counts() {
        let root = std::env::temp_dir().join(format!("aoe-progress-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("progress.json");
        let scope = Scope::new(Some(path.clone()));
        stage(Phase::PreparingSources);
        let value: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(value["phase"], "preparing_sources");
        assert!(value["completed"].is_null());
        count(Phase::BuildingPyramids, None, 8, 8, Unit::Pages);
        let final_bytes = fs::read(&path).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&final_bytes).unwrap();
        assert_eq!(value["completed"], 8);
        assert_eq!(value["total"], 8);
        assert!(!path.with_extension("tmp").exists());
        count(Phase::BuildingPyramids, None, 9, 8, Unit::Pages);
        assert_eq!(fs::read(&path).unwrap(), final_bytes);
        drop(scope);
        stage(Phase::SamplingWater);
        assert_eq!(fs::read(&path).unwrap(), final_bytes);
        assert_eq!(pyramid_page_count(65), 11);
        fs::remove_dir_all(root).unwrap();
    }
}
