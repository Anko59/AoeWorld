//! Bounded land survey used only to rank candidates. Passability components
//! are a preference, not a movement certificate: all starts still prove real
//! crossable routes with the unchanged area and exit checks.
use super::*;

pub(super) fn candidates(
    cache: &mut StartPassabilityCache<'_>,
    max_chunks: usize,
) -> Result<(Vec<TileCoord>, bool), EnvironmentPageError> {
    let config = cache.config;
    let center = TileCoord::new((config.width_tiles - 1) / 2, (config.height_tiles - 1) / 2);
    let center_chunk = TileCoord::new(center.x / CHUNK_TILES, center.y / CHUNK_TILES);
    let chunks_x = (config.width_tiles + CHUNK_TILES - 1) / CHUNK_TILES;
    let chunks_y = (config.height_tiles + CHUNK_TILES - 1) / CHUNK_TILES;
    let mut surveyed = BTreeSet::new();
    let mut land = BTreeSet::new();
    let max_ring = center_chunk
        .x
        .max(chunks_x - 1 - center_chunk.x)
        .max(center_chunk.y)
        .max(chunks_y - 1 - center_chunk.y);
    'survey: for ring in 0..=max_ring {
        for (x, y) in ring_chunks(center_chunk, ring) {
            if x < 0 || y < 0 || x >= chunks_x || y >= chunks_y {
                continue;
            }
            if surveyed.len() == max_chunks.min(START_SEARCH_CHUNKS) {
                break 'survey;
            }
            if (cache.cancelled)() {
                return Ok((Vec::new(), false));
            }
            // Loading a chunk once exposes its already composed passability.
            cache.passable(TileCoord::new(x * CHUNK_TILES, y * CHUNK_TILES))?;
            surveyed.insert((x, y));
            let tiles = &cache.entries[&(x, y)];
            for (index, passable) in tiles.iter().enumerate() {
                let tile = TileCoord::new(
                    x * CHUNK_TILES + index as i32 % CHUNK_TILES,
                    y * CHUNK_TILES + index as i32 / CHUNK_TILES,
                );
                if *passable && tile.x < config.width_tiles && tile.y < config.height_tiles {
                    land.insert(tile);
                }
            }
        }
    }
    let complete = surveyed.len() == (chunks_x as usize) * (chunks_y as usize);
    let mut regions = Vec::new();
    while let Some(origin) = land.pop_first() {
        let mut region = vec![origin];
        let mut pending = VecDeque::from([origin]);
        let mut boundary = false;
        let (mut low, mut high) = (origin, origin);
        while let Some(tile) = pending.pop_front() {
            if (cache.cancelled)() {
                return Ok((Vec::new(), false));
            }
            low.x = low.x.min(tile.x);
            low.y = low.y.min(tile.y);
            high.x = high.x.max(tile.x);
            high.y = high.y.max(tile.y);
            for (dx, dy) in [(0, -1), (-1, 0), (1, 0), (0, 1)] {
                let next = TileCoord::new(tile.x + dx, tile.y + dy);
                if next.x < 0
                    || next.y < 0
                    || next.x >= config.width_tiles
                    || next.y >= config.height_tiles
                {
                    continue;
                }
                boundary |= !surveyed.contains(&(next.x / CHUNK_TILES, next.y / CHUNK_TILES));
                if land.remove(&next) {
                    region.push(next);
                    pending.push_back(next);
                }
            }
        }
        // A fully surveyed component smaller than the required area or exit
        // diameter cannot certify a start anywhere. Do not spend the shared
        // exit budget repeatedly on the same island.
        let distance = START_EXIT_DISTANCE
            .min(((config.width_tiles.max(config.height_tiles) - 1) / 2).max(1) as u32);
        if !boundary
            && (region.len() < START_REACHABLE_TILES
                || (high.x.abs_diff(low.x) < distance && high.y.abs_diff(low.y) < distance))
        {
            continue;
        }
        region.sort_unstable_by_key(|tile| start_key(*tile, config));
        regions.push(region);
    }
    regions.sort_by_key(|region| {
        (
            std::cmp::Reverse(region.len()),
            start_key(region[0], config),
        )
    });
    // Try spatially separated clearings before neighboring tiles in each
    // region; the latter remain a fallback for narrow source-backed clearings.
    let mut result = Vec::new();
    for region in regions {
        result.extend(
            region
                .iter()
                .copied()
                .filter(|tile| tile == &center || (tile.x % 8 == 4 && tile.y % 8 == 4)),
        );
        result.extend(
            region
                .into_iter()
                .filter(|tile| *tile != center && !(tile.x % 8 == 4 && tile.y % 8 == 4)),
        );
    }
    Ok((result, complete))
}
