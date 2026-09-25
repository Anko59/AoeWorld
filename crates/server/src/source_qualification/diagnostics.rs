use crate::PageResidency;
use aoe_core::{TileCoord, WorldConfig};
use aoe_map::{
    CHUNK_TILES, ENVIRONMENT_PAGE_SAMPLES, EnvironmentPage, EnvironmentPageError,
    EnvironmentPageKey, EnvironmentPageProvider, MapPackage, PageLayer,
};
use aoe_simulation::Terrain;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

mod components;
use components::start_component_diagnostic;

const START_CLEAR_RADIUS: i32 = 2;
const START_COMPARISON_RADIUS: i32 = 1;
const START_REACHABLE_TILES: usize = 256;

pub(super) fn component_blocker_diagnostic(
    terrain: &Terrain,
    generator: &aoe_map::MapChunkGenerator,
    tiles: &BTreeSet<TileCoord>,
    config: WorldConfig,
) -> Result<String, aoe_map::EnvironmentPageError> {
    components::component_blocker_diagnostic(terrain, generator, tiles, config)
}

pub(super) fn start_diagnostic(
    terrain: &Terrain,
    generator: &aoe_map::MapChunkGenerator,
    package: &MapPackage,
    provider: &PageResidency,
    config: WorldConfig,
    max_chunks: usize,
    selected_start: Option<TileCoord>,
) -> Result<String, aoe_map::EnvironmentPageError> {
    let center = TileCoord::new((config.width_tiles - 1) / 2, (config.height_tiles - 1) / 2);
    let center_tile = generator
        .tile_at_with_cancel(center, &|| false)?
        .ok_or(EnvironmentPageError::Invalid)?;
    let center_object = generator.object_at_with_cancel(center, &|| false)?;
    let center_hyde = land_use_at(package, provider, center)?;
    let mut passable = BTreeMap::new();
    let mut center_clear_tiles = 0_usize;
    let mut center_clear_three_tiles = 0_usize;
    let mut center_objects = 0_usize;
    let mut cliffs = 0_usize;
    let mut neighbor_heights = Vec::new();
    let mut grid_heights = Vec::new();
    for y in (center.y - 2)..=(center.y + 2) {
        for x in (center.x - 2)..=(center.x + 2) {
            let coord = TileCoord::new(x, y);
            let clear = tile_passable(terrain, coord, config, &mut passable)?;
            center_clear_tiles += usize::from(clear);
            if x.abs_diff(center.x) <= START_COMPARISON_RADIUS as u32
                && y.abs_diff(center.y) <= START_COMPARISON_RADIUS as u32
            {
                center_clear_three_tiles += usize::from(clear);
            }
            if let Some(tile) = generator.tile_at_with_cancel(coord, &|| false)? {
                grid_heights.push((x, y, tile.geographic_height_centimeters));
                cliffs += usize::from(!tile.surface.walkable());
                if generator.object_at_with_cancel(coord, &|| false)?.is_some() {
                    center_objects += 1;
                }
                if x == center.x || y == center.y {
                    neighbor_heights.push((
                        x - center.x,
                        y - center.y,
                        tile.geographic_height_centimeters,
                    ));
                }
            }
        }
    }
    let mut max_neighbor_rise_cm = 0_i32;
    for left in &grid_heights {
        for right in &grid_heights {
            if (left.0 - right.0).abs() + (left.1 - right.1).abs() == 1 {
                max_neighbor_rise_cm = max_neighbor_rise_cm.max((left.2 - right.2).abs());
            }
        }
    }

    let chunks = scanned_chunks(config, center, max_chunks);
    let mut candidate_tiles = 0_usize;
    let mut clear_windows = 0_usize;
    let mut clear_three_windows = 0_usize;
    let mut reachable_windows = 0_usize;
    let mut reachable_three_windows = 0_usize;
    let mut maximum_reachable_start_tiles = 0_usize;
    let mut maximum_reachable_three_tiles = 0_usize;
    let mut land_use_candidates = 0_usize;
    let mut land_use_cleared_candidates = 0_usize;
    let mut eligible_start_candidates = Vec::new();
    for (chunk_x, chunk_y) in &chunks {
        let start_x = chunk_x * CHUNK_TILES;
        let start_y = chunk_y * CHUNK_TILES;
        for y in start_y..(start_y + CHUNK_TILES).min(config.height_tiles) {
            for x in start_x..(start_x + CHUNK_TILES).min(config.width_tiles) {
                let candidate = TileCoord::new(x, y);
                candidate_tiles += 1;
                let clear_three = has_clear_area(
                    terrain,
                    candidate,
                    config,
                    START_COMPARISON_RADIUS,
                    &mut passable,
                )?;
                let clear_five = has_clear_area(
                    terrain,
                    candidate,
                    config,
                    START_CLEAR_RADIUS,
                    &mut passable,
                )?;
                if clear_three {
                    clear_three_windows += 1;
                }
                if clear_five {
                    clear_windows += 1;
                }
                let reachable = if clear_three || clear_five {
                    Some(reachable_tiles(terrain, candidate, config, &mut passable)?)
                } else {
                    None
                };
                if clear_three {
                    let reachable = reachable.ok_or(EnvironmentPageError::Invalid)?;
                    maximum_reachable_three_tiles = maximum_reachable_three_tiles.max(reachable);
                    reachable_three_windows += usize::from(reachable >= START_REACHABLE_TILES);
                }
                if clear_five {
                    let reachable = reachable.ok_or(EnvironmentPageError::Invalid)?;
                    maximum_reachable_start_tiles = maximum_reachable_start_tiles.max(reachable);
                    if reachable >= START_REACHABLE_TILES {
                        reachable_windows += 1;
                        eligible_start_candidates.push(candidate);
                    }
                }
                let Some(tile) = generator.tile_at_with_cancel(candidate, &|| false)? else {
                    continue;
                };
                if !tile.passable {
                    continue;
                }
                if let Some((crop, grazing)) = land_use_at(package, provider, candidate)? {
                    land_use_candidates += 1;
                    land_use_cleared_candidates += usize::from(
                        generator
                            .is_tree_suppressed_by_historical_land_use(candidate, crop, grazing),
                    );
                }
            }
        }
    }

    let center_clearing_frequency = land_use_frequency(generator, center, center_hyde);
    let clearing_frequency_percent = if land_use_candidates == 0 {
        0.0
    } else {
        land_use_cleared_candidates as f64 * 100.0 / land_use_candidates as f64
    };
    let max_edge_grade_percent = f64::from(max_neighbor_rise_cm) / 2.0;
    let component_diagnostic = start_component_diagnostic(
        terrain,
        generator,
        package,
        &eligible_start_candidates,
        selected_start,
        TileCoord::new(config.width_tiles - 2, (config.height_tiles - 1) / 2),
        config,
    )?;
    Ok(format!(
        "center=({},{}), center_clear_5x5_tiles={center_clear_tiles}/25, center_clear_3x3_tiles={center_clear_three_tiles}/9, center_objects={center_objects}, cliffs={cliffs}, center_material={:?}, center_biome={:?}, center_water={:?}, center_passable={}, center_surface={:?}, center_object={:?}, center_height_cm={}, cardinal_sample_heights={neighbor_heights:?}, max_adjacent_rise_cm={max_neighbor_rise_cm}, max_2m_edge_grade_percent={max_edge_grade_percent:.2}, scanned_chunks={}, candidate_tiles={candidate_tiles}, fully_clear_5x5_windows={clear_windows}, clear_5x5_windows_reaching_256={reachable_windows}, fully_clear_3x3_windows={clear_three_windows}, clear_3x3_windows_reaching_256={reachable_three_windows}, max_reachable_tiles_from_clear_5x5_window={maximum_reachable_start_tiles}, max_reachable_tiles_from_clear_3x3_window={maximum_reachable_three_tiles}, center_hyde_crop_pct={:?}, center_hyde_grazing_pct={:?}, center_hyde_clears_tree={center_clearing_frequency:?}, scanned_hyde_cleared_tiles={land_use_cleared_candidates}/{land_use_candidates}, scanned_hyde_clearing_frequency_percent={clearing_frequency_percent:.3}, start_component_diagnostic={component_diagnostic}",
        center.x,
        center.y,
        center_tile.material,
        center_tile.biome,
        center_tile.water,
        center_tile.passable,
        center_tile.surface,
        center_object.map(|node| (node.object, node.id)),
        center_tile.geographic_height_centimeters,
        chunks.len(),
        center_hyde.map(|land_use| land_use.0),
        center_hyde.map(|land_use| land_use.1),
    ))
}

fn tile_passable(
    terrain: &Terrain,
    tile: TileCoord,
    config: WorldConfig,
    cache: &mut BTreeMap<TileCoord, bool>,
) -> Result<bool, EnvironmentPageError> {
    if let Some(passable) = cache.get(&tile) {
        return Ok(*passable);
    }
    let passable = terrain.passable_with_cancel(tile, config, &|| false)?;
    cache.insert(tile, passable);
    Ok(passable)
}

fn has_clear_area(
    terrain: &Terrain,
    center: TileCoord,
    config: WorldConfig,
    radius: i32,
    cache: &mut BTreeMap<TileCoord, bool>,
) -> Result<bool, EnvironmentPageError> {
    for offset_y in -radius..=radius {
        for offset_x in -radius..=radius {
            if !tile_passable(
                terrain,
                TileCoord::new(center.x + offset_x, center.y + offset_y),
                config,
                cache,
            )? {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

fn reachable_tiles(
    terrain: &Terrain,
    origin: TileCoord,
    config: WorldConfig,
    passable: &mut BTreeMap<TileCoord, bool>,
) -> Result<usize, EnvironmentPageError> {
    let mut visited = BTreeSet::from([origin]);
    let mut pending = VecDeque::from([origin]);
    while let Some(tile) = pending.pop_front() {
        if visited.len() >= START_REACHABLE_TILES {
            break;
        }
        for dy in -1..=1 {
            for dx in -1..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let next = TileCoord::new(tile.x + dx, tile.y + dy);
                if visited.contains(&next)
                    || !tile_passable(terrain, next, config, passable)?
                    || !terrain.crossable_with_cancel(tile, next, config, &|| false)?
                {
                    continue;
                }
                visited.insert(next);
                pending.push_back(next);
            }
        }
    }
    Ok(visited.len())
}

fn scanned_chunks(config: WorldConfig, center: TileCoord, max_chunks: usize) -> Vec<(i32, i32)> {
    let center_chunk = TileCoord::new(
        center.x.div_euclid(CHUNK_TILES),
        center.y.div_euclid(CHUNK_TILES),
    );
    let chunks_x = config.width_tiles.saturating_add(CHUNK_TILES - 1) / CHUNK_TILES;
    let chunks_y = config.height_tiles.saturating_add(CHUNK_TILES - 1) / CHUNK_TILES;
    let max_ring = center_chunk
        .x
        .max(chunks_x - 1 - center_chunk.x)
        .max(center_chunk.y)
        .max(chunks_y - 1 - center_chunk.y);
    let mut chunks = Vec::new();
    for ring in 0..=max_ring {
        for (x, y) in ring_chunks(center_chunk, ring) {
            if x < 0 || y < 0 || x >= chunks_x || y >= chunks_y {
                continue;
            }
            if chunks.len() == max_chunks {
                return chunks;
            }
            chunks.push((x, y));
        }
    }
    chunks
}

fn ring_chunks(center: TileCoord, ring: i32) -> Vec<(i32, i32)> {
    if ring == 0 {
        return vec![(center.x, center.y)];
    }
    let left = center.x - ring;
    let right = center.x + ring;
    let top = center.y - ring;
    let bottom = center.y + ring;
    let mut chunks = Vec::with_capacity((ring as usize) * 8);
    for x in left..=right {
        chunks.push((x, top));
    }
    for y in top + 1..bottom {
        chunks.extend([(left, y), (right, y)]);
    }
    for x in left..=right {
        chunks.push((x, bottom));
    }
    chunks
}

fn land_use_frequency(
    generator: &aoe_map::MapChunkGenerator,
    tile: TileCoord,
    land_use: Option<(u8, u8)>,
) -> Option<bool> {
    land_use.map(|(crop, grazing)| {
        generator.is_tree_suppressed_by_historical_land_use(tile, crop, grazing)
    })
}

fn land_use_at(
    package: &MapPackage,
    provider: &PageResidency,
    tile: TileCoord,
) -> Result<Option<(u8, u8)>, EnvironmentPageError> {
    if package.environment.historical_land_use.is_none() {
        return Ok(None);
    }
    let samples = package.environment.samples_per_axis;
    let (source_x, source_y) =
        nearest_source_coordinate(tile, samples, package.estimate.tiles_per_side as i32)?;
    let key = EnvironmentPageKey {
        layer: PageLayer::HistoricalLandUse,
        level: 0,
        x: source_x / u16::from(ENVIRONMENT_PAGE_SAMPLES),
        y: source_y / u16::from(ENVIRONMENT_PAGE_SAMPLES),
    };
    let page = provider.page(key, &|| false)?;
    let page = match page.as_ref() {
        EnvironmentPage::HistoricalLandUse(page) => page,
        _ => return Err(EnvironmentPageError::Corrupt),
    };
    let local_x = usize::from(source_x % u16::from(ENVIRONMENT_PAGE_SAMPLES));
    let local_y = usize::from(source_y % u16::from(ENVIRONMENT_PAGE_SAMPLES));
    let index = (local_x < usize::from(page.width) && local_y < usize::from(page.height))
        .then_some(local_y * usize::from(page.width) + local_x)
        .ok_or(EnvironmentPageError::Corrupt)?;
    Ok(Some((
        page.crop_percent[index],
        page.grazing_percent[index],
    )))
}

fn nearest_source_coordinate(
    tile: TileCoord,
    samples: u16,
    width_tiles: i32,
) -> Result<(u16, u16), EnvironmentPageError> {
    let tile_axis = u64::try_from(
        width_tiles
            .checked_sub(1)
            .ok_or(EnvironmentPageError::Invalid)?,
    )
    .map_err(|_| EnvironmentPageError::Invalid)?;
    let source_axis = u64::from(
        samples
            .checked_sub(1)
            .ok_or(EnvironmentPageError::Invalid)?,
    );
    if tile_axis == 0 {
        return Ok((0, 0));
    }
    let x = u64::try_from(tile.x.clamp(0, width_tiles.saturating_sub(1)))
        .map_err(|_| EnvironmentPageError::Invalid)?;
    let y = u64::try_from(tile.y.clamp(0, width_tiles.saturating_sub(1)))
        .map_err(|_| EnvironmentPageError::Invalid)?;
    let source_x = u16::try_from((x * source_axis + tile_axis / 2) / tile_axis)
        .map_err(|_| EnvironmentPageError::Invalid)?;
    let source_y = u16::try_from((y * source_axis + tile_axis / 2) / tile_axis)
        .map_err(|_| EnvironmentPageError::Invalid)?;
    Ok((source_x, source_y))
}
