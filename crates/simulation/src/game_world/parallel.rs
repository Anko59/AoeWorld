//! Fork-join helpers for the deterministic parallel tick. Every helper's
//! result is independent of the thread count: tasks are pure over a snapshot
//! (`par_map`) or own disjoint data (`par_each_mut`). WebAssembly has no
//! threads and runs the same code in order.

/// Movers (or other items) per work unit. A fixed constant: it only sets the
/// task granularity, never a result.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) const SHARD: usize = 512;

/// `f(0..count)` collected in index order.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn par_map<T: Send>(count: usize, f: impl Fn(usize) -> T + Sync + Send) -> Vec<T> {
    use rayon::prelude::*;
    if count < 2 * SHARD {
        return (0..count).map(f).collect();
    }
    (0..count)
        .into_par_iter()
        .with_min_len(SHARD)
        .map(f)
        .collect()
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn par_map<T: Send>(count: usize, f: impl Fn(usize) -> T + Sync + Send) -> Vec<T> {
    (0..count).map(f).collect()
}

/// Runs `f` on every item, each on its own task.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn par_each_mut<T: Send>(items: &mut [T], f: impl Fn(&mut T) + Sync + Send) {
    use rayon::prelude::*;
    items.par_iter_mut().for_each(f);
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn par_each_mut<T: Send>(items: &mut [T], f: impl Fn(&mut T) + Sync + Send) {
    items.iter_mut().for_each(f);
}
