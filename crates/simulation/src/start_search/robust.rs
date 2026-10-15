//! Map-wide coarse valley survey, followed by progressively relaxed certificates.
//! The survey has at most 4,225 probes. The rare exact fallback examines each
//! possible footprint once: a finite map-area budget, cancellable at every tile.
use super::*;
use aoe_map::{GroundMaterial, WaterKind};

const AXIS_PROBES: i32 = 64;
const CANDIDATE_BUDGET: usize = 512;

pub(super) fn search(
    cache: &mut StartPassabilityCache<'_>,
) -> Result<StartSearchResult, EnvironmentPageError> {
    match survey(cache) {
        Err(EnvironmentPageError::Cancelled) => Ok(StartSearchResult::Cancelled),
        result => result,
    }
}

fn survey(
    cache: &mut StartPassabilityCache<'_>,
) -> Result<StartSearchResult, EnvironmentPageError> {
    let config = cache.config;
    let stride =
        ((config.width_tiles.max(config.height_tiles) + AXIS_PROBES - 1) / AXIS_PROBES).max(1);
    let mut land = BTreeSet::new();
    for y in (0..config.height_tiles).step_by(stride as usize) {
        for x in (0..config.width_tiles).step_by(stride as usize) {
            if (cache.cancelled)() {
                return Ok(StartSearchResult::Cancelled);
            }
            let tile = TileCoord::new(x, y);
            if probe(cache, tile)? {
                land.insert(tile);
            }
        }
    }
    // The deterministic central forest opening can lie between lattice
    // stations even on the largest virtual maps. Always examine it directly.
    let center = TileCoord::new((config.width_tiles - 1) / 2, (config.height_tiles - 1) / 2);
    if probe(cache, center)? {
        land.insert(center);
    }
    if let Some(preferred) = cache.preferred {
        land.insert(preferred);
    }
    let mut qualities = BTreeMap::new();
    let (mut low, mut high) = (i32::MAX, i32::MIN);
    for &tile in &land {
        let value = quality(cache, tile)?;
        low = low.min(value.2);
        high = high.max(value.2);
        qualities.insert(tile, value);
    }
    // Relief above 500 m denotes a mountainous landscape. Prefer the lower
    // third before comparing component sizes; a broad summit plateau must
    // not beat a smaller usable valley simply because it has more flat tiles.
    let lowland_limit = if high.saturating_sub(low) > 50_000 {
        low.saturating_add(high.saturating_sub(low) / 3)
    } else {
        i32::MAX
    };
    let lowland = |tile: &TileCoord| {
        qualities
            .get(tile)
            .is_some_and(|value| value.2 <= lowland_limit)
    };
    let mut regions = Vec::new();
    while let Some(origin) = land.pop_first() {
        let mut members = vec![origin];
        let mut queue = VecDeque::from([origin]);
        while let Some(tile) = queue.pop_front() {
            for (dx, dy) in [(0, -stride), (-stride, 0), (stride, 0), (0, stride)] {
                let next = TileCoord::new(tile.x + dx, tile.y + dy);
                if land.contains(&next)
                    && lowland(&next) == lowland(&origin)
                    && line_clear(cache, tile, next)?
                {
                    land.remove(&next);
                    members.push(next);
                    queue.push_back(next);
                }
            }
        }
        // Largest connected sampled lowland first; low elevation, gentle ground
        // and centre distance break ties deterministically inside each valley.
        let mut ranked = Vec::new();
        for tile in members.iter().copied() {
            ranked.push((qualities[&tile], tile));
        }
        ranked.sort_unstable_by_key(|(quality, tile)| (*quality, start_key(*tile, config)));
        regions.push((lowland(&origin), members.len(), ranked));
    }
    regions.sort_by_key(|(lowland, size, members)| {
        (
            !*lowland,
            std::cmp::Reverse(*size),
            members[0].0,
            start_key(members[0].1, config),
        )
    });
    let share = (CANDIDATE_BUDGET / regions.len().max(1)).max(1);
    // Give each valley a share: an unsuitable large region cannot consume
    // the certificate budget reserved for smaller viable regions.
    for (_, _, members) in regions.into_iter().take(CANDIDATE_BUDGET) {
        let candidates: Vec<_> = members
            .into_iter()
            .take(share)
            .map(|(_, tile)| tile)
            .collect();
        // Strict starts have space for an economy. Relax water distance,
        // slope and region size without changing physical passability.
        for (radius, required, inland, max_slope) in [
            (3, 256, true, 0),
            (2, 64, false, 1),
            (0, 4, false, i32::MAX),
        ] {
            for &tile in &candidates {
                if (cache.cancelled)() {
                    return Ok(StartSearchResult::Cancelled);
                }
                let (shore, slope, _) = quality(cache, tile)?;
                if (inland && shore) || slope > max_slope {
                    continue;
                }
                let origin = TileCoord::new(tile.x - 2, tile.y - 2);
                if footprint(cache, origin, 4)?
                    && (radius == 0 || clearing(cache, tile, radius)?)
                    && room(cache, origin, required)?
                {
                    return Ok(StartSearchResult::Found(origin));
                }
            }
        }
    }
    // A coarse survey can miss a narrow bowl or an off-grid clearing. Never
    // call that proof of absence, and never alter the source landscape.
    for y in 0..config.height_tiles.saturating_sub(3) {
        for x in 0..config.width_tiles.saturating_sub(3) {
            if (cache.cancelled)() {
                return Ok(StartSearchResult::Cancelled);
            }
            let origin = TileCoord::new(x, y);
            if footprint(cache, origin, 4)? && room(cache, origin, 4)? {
                return Ok(StartSearchResult::Found(origin));
            }
        }
    }
    Ok(StartSearchResult::Unavailable)
}

fn probe(
    cache: &mut StartPassabilityCache<'_>,
    tile: TileCoord,
) -> Result<bool, EnvironmentPageError> {
    if (cache.cancelled)() {
        return Err(EnvironmentPageError::Cancelled);
    }
    if tile.x < 0
        || tile.y < 0
        || tile.x >= cache.config.width_tiles
        || tile.y >= cache.config.height_tiles
    {
        return Ok(false);
    }
    if let Some(chunk) = cache
        .entries
        .get(&(tile.x / CHUNK_TILES, tile.y / CHUNK_TILES))
    {
        return Ok(chunk[(tile.y % CHUNK_TILES * CHUNK_TILES + tile.x % CHUNK_TILES) as usize]);
    }
    if let Some(passable) = cache.point_samples.get(&tile) {
        return Ok(*passable);
    }
    let passable = cache
        .terrain
        .passable_with_cancel(tile, cache.config, cache.cancelled)?;
    if cache.point_samples.len() == 131_072 {
        cache.point_samples.clear();
    }
    cache.point_samples.insert(tile, passable);
    Ok(passable)
}

fn line_clear(
    cache: &mut StartPassabilityCache<'_>,
    from: TileCoord,
    to: TileCoord,
) -> Result<bool, EnvironmentPageError> {
    // This is a bounded connectivity preference, not a movement route.
    // Sample at most eight stations even when game maps span many kilometres.
    let distance = (to.x - from.x).abs().max((to.y - from.y).abs());
    for step in 1..=distance.min(8) {
        let count = distance.min(8);
        let next = TileCoord::new(
            from.x + (to.x - from.x) * step / count,
            from.y + (to.y - from.y) * step / count,
        );
        let prior = TileCoord::new(
            next.x - (to.x - from.x).signum(),
            next.y - (to.y - from.y).signum(),
        );
        if !probe(cache, next)?
            || !probe(cache, prior)?
            || !cache
                .terrain
                .crossable_with_cancel(prior, next, cache.config, cache.cancelled)?
        {
            return Ok(false);
        }
    }
    Ok(true)
}

fn quality(
    cache: &StartPassabilityCache<'_>,
    tile: TileCoord,
) -> Result<(bool, i32, i32), EnvironmentPageError> {
    let Terrain::Map { generator, .. } = cache.terrain else {
        return Ok((false, 0, 0));
    };
    let Some(sample) = generator.tile_at_with_cancel(tile, cache.cancelled)? else {
        return Ok((true, i32::MAX, i32::MAX));
    };
    let levels = sample.surface.corner_game_height_levels;
    let slope = i32::from(*levels.iter().max().unwrap_or(&0))
        - i32::from(*levels.iter().min().unwrap_or(&0));
    Ok((
        sample.water != WaterKind::None
            || matches!(
                sample.material,
                GroundMaterial::Sand | GroundMaterial::Shore
            ),
        slope,
        sample.geographic_height_centimeters,
    ))
}

pub(super) fn footprint(
    cache: &mut StartPassabilityCache<'_>,
    origin: TileCoord,
    side: i32,
) -> Result<bool, EnvironmentPageError> {
    for y in origin.y..origin.y + side {
        for x in origin.x..origin.x + side {
            if !probe(cache, TileCoord::new(x, y))? {
                return Ok(false);
            }
        }
    }
    Ok(true)
}
fn clearing(
    cache: &mut StartPassabilityCache<'_>,
    center: TileCoord,
    radius: i32,
) -> Result<bool, EnvironmentPageError> {
    footprint(
        cache,
        TileCoord::new(center.x - radius, center.y - radius),
        radius * 2 + 1,
    )
}
fn room(
    cache: &mut StartPassabilityCache<'_>,
    origin: TileCoord,
    required: usize,
) -> Result<bool, EnvironmentPageError> {
    let inside = |tile: TileCoord| {
        tile.x >= origin.x && tile.y >= origin.y && tile.x < origin.x + 4 && tile.y < origin.y + 4
    };
    let mut starts = Vec::new();
    for y in origin.y - 1..=origin.y + 4 {
        for x in origin.x - 1..=origin.x + 4 {
            let tile = TileCoord::new(x, y);
            if !inside(tile) && probe(cache, tile)? {
                starts.push(tile);
            }
        }
    }
    for start in starts {
        let mut seen = BTreeSet::from([start]);
        let mut queue = VecDeque::from([start]);
        while let Some(tile) = queue.pop_front() {
            if seen.len() >= required {
                return Ok(true);
            }
            for (dx, dy) in [(0, -1), (-1, 0), (1, 0), (0, 1)] {
                let next = TileCoord::new(tile.x + dx, tile.y + dy);
                if !inside(next)
                    && !seen.contains(&next)
                    && probe(cache, next)?
                    && cache.terrain.crossable_with_cancel(
                        tile,
                        next,
                        cache.config,
                        cache.cancelled,
                    )?
                {
                    seen.insert(next);
                    queue.push_back(next);
                }
            }
        }
    }
    Ok(false)
}
