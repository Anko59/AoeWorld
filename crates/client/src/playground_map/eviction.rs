use super::{CachedChunk, Camera, ChunkTable, chunk_distance_for, chunk_resident_bytes, heights};

type RankedChunk = ((bool, bool), f64, (i32, i32));

#[inline(never)]
pub(super) fn sort_chunks(chunks: &mut [RankedChunk]) {
    chunks.sort_unstable_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)).then(a.2.cmp(&b.2)));
}

pub(crate) fn evict_distant_chunks_with_limits(
    chunks: &mut ChunkTable<CachedChunk>,
    camera: Camera,
    maximum_chunks: usize,
    maximum_bytes: usize,
    preferred: &[(i32, i32)],
) -> (bool, Option<(i16, i16)>) {
    let mut cached_bytes = chunks.values().map(chunk_resident_bytes).sum::<usize>();
    if chunks.len() <= maximum_chunks && cached_bytes <= maximum_bytes {
        return (false, None);
    }
    // Membership only: a sorted compact list avoids a second coordinate tree.
    let mut preferred = preferred.to_vec();
    preferred.sort_unstable();
    let mut candidates: Vec<_> = chunks
        .keys()
        .copied()
        .map(|key| {
            let offscreen = preferred.binary_search(&key).is_err();
            ((offscreen, false), chunk_distance_for(key, camera), key)
        })
        .collect();
    sort_chunks(&mut candidates);
    let mut removed = false;
    for (_, _, coordinate) in candidates.into_iter().rev() {
        if chunks.len() <= maximum_chunks && cached_bytes <= maximum_bytes {
            break;
        }
        if let Some(chunk) = chunks.remove(&coordinate) {
            cached_bytes = cached_bytes.saturating_sub(chunk_resident_bytes(&chunk));
            removed = true;
        }
    }
    let resident_bounds = removed.then(|| heights::resident_height_bounds(chunks.values()));
    (removed, resident_bounds.flatten())
}
