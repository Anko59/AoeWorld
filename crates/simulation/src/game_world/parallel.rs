//! Fork-join helpers for the deterministic parallel tick. Every helper's
//! result is independent of the thread count: tasks are pure over a snapshot
//! (`par_map`) or own disjoint data (`par_each_mut`). WebAssembly has no
//! threads and runs the same code in order.

/// Movers (or other items) per work unit. A fixed constant: it only sets the
/// task granularity, never a result.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) const SHARD: usize = 128;

/// `f(0..count)` collected in index order.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn par_map_into<T: Send>(
    count: usize,
    out: &mut Vec<T>,
    f: impl Fn(usize) -> T + Sync + Send,
) {
    use rayon::prelude::*;
    if count < 2 * SHARD || rayon::current_num_threads() == 1 {
        out.clear();
        out.extend((0..count).map(f));
        return;
    }
    (0..count)
        .into_par_iter()
        .with_min_len(SHARD)
        .map(f)
        .collect_into_vec(out);
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn par_map_into<T: Send>(
    count: usize,
    out: &mut Vec<T>,
    f: impl Fn(usize) -> T + Sync + Send,
) {
    out.clear();
    out.extend((0..count).map(f));
}

/// Runs `f` on every item, each on its own task.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn par_each_mut<T: Send>(items: &mut [T], f: impl Fn(&mut T) + Sync + Send) {
    use rayon::prelude::*;
    if rayon::current_num_threads() == 1 {
        items.iter_mut().for_each(f);
    } else {
        items.par_iter_mut().for_each(f);
    }
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn par_each_mut<T: Send>(items: &mut [T], f: impl Fn(&mut T) + Sync + Send) {
    items.iter_mut().for_each(f);
}

impl crate::GameWorld {
    /// Diagnostic pool size; timing reports distinguish quota from workers.
    pub fn movement_worker_count(&self) -> usize {
        #[cfg(not(target_arch = "wasm32"))]
        {
            rayon::current_num_threads()
        }
        #[cfg(target_arch = "wasm32")]
        {
            1
        }
    }
}
