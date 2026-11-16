//! Replay-only census of work inside the fork-join helpers. Off unless a
//! reader is installed; the single-worker replay then runs every region inline
//! on the counted thread, so a before/after read of the thread's instruction
//! counter gives the instructions a wide pool could have shared. Nothing here
//! reaches simulation state.
use std::{
    collections::BTreeMap,
    panic::Location,
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

type Reader = Arc<dyn Fn() -> u64 + Send + Sync>;
static READER: RwLock<Option<Reader>> = RwLock::new(None);
static ENABLED: AtomicBool = AtomicBool::new(false);
static SUPPRESSED: AtomicBool = AtomicBool::new(false);
static INSIDE: AtomicU64 = AtomicU64::new(0);
static SITES: Mutex<BTreeMap<String, Site>> = Mutex::new(BTreeMap::new());

/// One call site's totals since the last drain.
#[derive(Clone, Debug, Default)]
pub struct Site {
    pub calls: u64,
    /// Calls too small for a pool to split (below `2 * SHARD`, or under 2 items).
    pub below_threshold: u64,
    pub items: u64,
    pub instructions: u64,
    /// Sum over calls of the largest single task (chunk or item).
    pub largest_task: u64,
}

pub fn install(reader: impl Fn() -> u64 + Send + Sync + 'static) {
    *READER.write().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(reader));
    ENABLED.store(true, Ordering::Release);
}
/// Turns the census off for a whole replay, so a one-worker run's wall time
/// carries no per-task counter reads and compares fairly with wider pools.
pub fn suppress(off: bool) {
    SUPPRESSED.store(off, Ordering::Release);
}
pub fn uninstall() {
    ENABLED.store(false, Ordering::Release);
    *READER.write().unwrap_or_else(|e| e.into_inner()) = None;
}
/// Instructions spent inside regions so far; callers difference it.
pub fn inside_total() -> u64 {
    INSIDE.load(Ordering::Relaxed)
}
pub fn drain() -> BTreeMap<String, Site> {
    std::mem::take(&mut *SITES.lock().unwrap_or_else(|e| e.into_inner()))
}

pub(super) fn reader() -> Option<Reader> {
    if ENABLED.load(Ordering::Acquire)
        && !SUPPRESSED.load(Ordering::Acquire)
        && rayon::current_num_threads() == 1
    {
        READER.read().unwrap_or_else(|e| e.into_inner()).clone()
    } else {
        None
    }
}

pub(super) struct Region {
    read: Reader,
    site: &'static Location<'static>,
    items: usize,
    below: bool,
    started: u64,
    largest: u64,
}
impl Region {
    pub(super) fn begin(
        read: Reader,
        site: &'static Location<'static>,
        items: usize,
        below: bool,
    ) -> Self {
        let started = read();
        Self {
            read,
            site,
            items,
            below,
            started,
            largest: 0,
        }
    }
    /// Runs one task (a chunk or an item), keeping the largest.
    pub(super) fn task<R>(&mut self, f: impl FnOnce() -> R) -> R {
        let before = (self.read)();
        let out = f();
        self.largest = self.largest.max((self.read)().saturating_sub(before));
        out
    }
    pub(super) fn end(self) {
        let spent = (self.read)().saturating_sub(self.started);
        INSIDE.fetch_add(spent, Ordering::Relaxed);
        let mut sites = SITES.lock().unwrap_or_else(|e| e.into_inner());
        let row = sites
            .entry(format!("{}:{}", self.site.file(), self.site.line()))
            .or_default();
        row.calls += 1;
        row.below_threshold += u64::from(self.below);
        row.items += self.items as u64;
        row.instructions += spent;
        row.largest_task += self.largest;
    }
}
