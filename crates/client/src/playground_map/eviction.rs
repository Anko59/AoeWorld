use super::{Camera, Chunk, chunk_distance_for, chunk_resident_bytes, heights};
use aoe_core::WorldConfig;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn evict_distant_chunks_with_limits(
    chunks: &mut BTreeMap<(i32, i32), Chunk>,
    discovered: &mut BTreeSet<(i32, i32)>,
    camera: Camera,
    config: WorldConfig,
    maximum_chunks: usize,
    maximum_bytes: usize,
) -> (bool, Option<(i16, i16)>) {
    let mut cached_bytes = chunks.values().map(chunk_resident_bytes).sum::<usize>();
    if chunks.len() <= maximum_chunks && cached_bytes <= maximum_bytes {
        return (false, None);
    }
    let mut coordinates = chunks.keys().copied().collect::<Vec<_>>();
    coordinates.sort_by(|left, right| {
        let left_distance = chunk_distance_for(*left, camera, config);
        let right_distance = chunk_distance_for(*right, camera, config);
        right_distance
            .total_cmp(&left_distance)
            .then(right.cmp(left))
    });
    let mut removed = false;
    for coordinate in coordinates {
        if chunks.len() <= maximum_chunks && cached_bytes <= maximum_bytes {
            break;
        }
        if let Some(chunk) = chunks.remove(&coordinate) {
            cached_bytes = cached_bytes.saturating_sub(chunk_resident_bytes(&chunk));
            discovered.remove(&coordinate);
            removed = true;
        }
    }
    let resident_bounds = removed.then(|| heights::resident_height_bounds(chunks.values()));
    (removed, resident_bounds.flatten())
}
