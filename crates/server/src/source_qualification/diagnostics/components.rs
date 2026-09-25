use aoe_core::{TileCoord, WorldConfig};
use aoe_map::{
    EnvironmentPageError, MapChunkGenerator, MapPackage, ObjectKind, SurfaceKind, WaterKind,
};
use aoe_simulation::Terrain;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

const MAX_COMPONENT_DIAGNOSTIC_TILES: usize = 65_536;
type BlockerCounts = BTreeMap<&'static str, usize>;

struct ComponentSummary {
    representative: TileCoord,
    eligible_candidates: usize,
    tile_count: usize,
    truncated: bool,
    bounds: (i32, i32, i32, i32),
    selected_start: &'static str,
    east_endpoint: &'static str,
    biomes: BTreeMap<String, usize>,
    boundary_edges: BTreeMap<&'static str, usize>,
    blocker_tiles: BTreeMap<&'static str, usize>,
}

pub(super) fn start_component_diagnostic(
    terrain: &Terrain,
    generator: &MapChunkGenerator,
    package: &MapPackage,
    candidates: &[TileCoord],
    selected_start: Option<TileCoord>,
    east_endpoint: TileCoord,
    config: WorldConfig,
) -> Result<String, EnvironmentPageError> {
    let mut component_for_tile = BTreeMap::<TileCoord, usize>::new();
    let mut components: Vec<ComponentSummary> = Vec::new();
    for candidate in candidates {
        if let Some(component) = component_for_tile.get(candidate).copied() {
            components[component].eligible_candidates += 1;
            continue;
        }
        let mut visited = BTreeSet::from([*candidate]);
        let mut pending = VecDeque::from([*candidate]);
        let mut truncated = false;
        while let Some(tile) = pending.pop_front() {
            if visited.len() >= MAX_COMPONENT_DIAGNOSTIC_TILES {
                truncated = true;
                break;
            }
            for dy in -1..=1 {
                for dx in -1..=1 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let next = TileCoord::new(tile.x + dx, tile.y + dy);
                    if visited.contains(&next)
                        || !terrain.passable_with_cancel(next, config, &|| false)?
                        || !terrain.crossable_with_cancel(tile, next, config, &|| false)?
                    {
                        continue;
                    }
                    visited.insert(next);
                    pending.push_back(next);
                }
            }
        }
        let min_x = visited
            .iter()
            .map(|tile| tile.x)
            .min()
            .unwrap_or(candidate.x);
        let max_x = visited
            .iter()
            .map(|tile| tile.x)
            .max()
            .unwrap_or(candidate.x);
        let min_y = visited
            .iter()
            .map(|tile| tile.y)
            .min()
            .unwrap_or(candidate.y);
        let max_y = visited
            .iter()
            .map(|tile| tile.y)
            .max()
            .unwrap_or(candidate.y);
        let selected_start_status = selected_start.map_or("not_applicable", |start| {
            if visited.contains(&start) {
                "yes"
            } else if truncated {
                "unknown_truncated"
            } else {
                "no"
            }
        });
        let east_endpoint_status = if visited.contains(&east_endpoint) {
            "yes"
        } else if truncated {
            "unknown_truncated"
        } else {
            "no"
        };
        let biomes = biome_counts(generator, &visited)?;
        let (boundary_edges, blocker_tiles) =
            boundary_counts(terrain, generator, &visited, config)?;
        let component_index = components.len();
        for tile in &visited {
            component_for_tile.insert(*tile, component_index);
        }
        components.push(ComponentSummary {
            representative: *candidate,
            eligible_candidates: 1,
            tile_count: visited.len(),
            truncated,
            bounds: (min_x, max_x, min_y, max_y),
            selected_start: selected_start_status,
            east_endpoint: east_endpoint_status,
            biomes,
            boundary_edges,
            blocker_tiles,
        });
    }
    let total = candidates.len();
    let connected_to_start = components
        .iter()
        .filter(|component| component.selected_start == "yes")
        .map(|component| component.eligible_candidates)
        .sum::<usize>();
    let connected_to_east = components
        .iter()
        .filter(|component| component.east_endpoint == "yes")
        .map(|component| component.eligible_candidates)
        .sum::<usize>();
    let worldcover = worldcover_status(package);
    let summaries = components
        .iter()
        .map(|component| {
            format!(
                "root=({},{}),eligible={},tiles={},bounds={}..{},{}..{},start={},east={}{}{}",
                component.representative.x,
                component.representative.y,
                component.eligible_candidates,
                component.tile_count,
                component.bounds.0,
                component.bounds.1,
                component.bounds.2,
                component.bounds.3,
                component.selected_start,
                component.east_endpoint,
                if component.truncated {
                    ",truncated"
                } else {
                    ""
                },
                if component.selected_start == "yes" {
                    format!(
                        ",biomes={:?},blocked_frontier_edges={:?},distinct_blocker_tiles={:?}",
                        component.biomes, component.boundary_edges, component.blocker_tiles
                    )
                } else {
                    String::new()
                },
            )
        })
        .collect::<Vec<_>>();
    Ok(format!(
        "eligible_5x5_candidates={total}, candidates_in_selected_start_component={connected_to_start}, candidates_connected_to_east_endpoint={connected_to_east}, local_worldcover={worldcover}, components=[{}]",
        summaries.join(";")
    ))
}

pub(super) fn component_blocker_diagnostic(
    terrain: &Terrain,
    generator: &MapChunkGenerator,
    tiles: &BTreeSet<TileCoord>,
    config: WorldConfig,
) -> Result<String, EnvironmentPageError> {
    let (edge_counts, blocker_tiles) = boundary_counts(terrain, generator, tiles, config)?;
    Ok(format!(
        "blocked_frontier_edges={edge_counts:?},distinct_blocker_tiles={blocker_tiles:?}"
    ))
}

fn biome_counts(
    generator: &MapChunkGenerator,
    tiles: &BTreeSet<TileCoord>,
) -> Result<BTreeMap<String, usize>, EnvironmentPageError> {
    let mut counts = BTreeMap::new();
    for tile in tiles {
        let biome = generator
            .tile_at_with_cancel(*tile, &|| false)?
            .ok_or(EnvironmentPageError::Invalid)?
            .biome;
        *counts.entry(format!("{biome:?}")).or_insert(0) += 1;
    }
    Ok(counts)
}

fn boundary_counts(
    terrain: &Terrain,
    generator: &MapChunkGenerator,
    tiles: &BTreeSet<TileCoord>,
    config: WorldConfig,
) -> Result<(BlockerCounts, BlockerCounts), EnvironmentPageError> {
    let mut edge_counts = BTreeMap::new();
    let mut blocker_cells = BTreeMap::<TileCoord, &'static str>::new();
    for tile in tiles {
        for dy in -1..=1 {
            for dx in -1..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let next = TileCoord::new(tile.x + dx, tile.y + dy);
                if tiles.contains(&next) {
                    continue;
                }
                let reason = blocker_reason(terrain, generator, *tile, next, config)?;
                *edge_counts.entry(reason).or_insert(0) += 1;
                blocker_cells.entry(next).or_insert(reason);
            }
        }
    }
    let mut tile_counts = BTreeMap::new();
    for reason in blocker_cells.into_values() {
        *tile_counts.entry(reason).or_insert(0) += 1;
    }
    Ok((edge_counts, tile_counts))
}

fn blocker_reason(
    terrain: &Terrain,
    generator: &MapChunkGenerator,
    from: TileCoord,
    to: TileCoord,
    config: WorldConfig,
) -> Result<&'static str, EnvironmentPageError> {
    if to.x < 0 || to.y < 0 || to.x >= config.width_tiles || to.y >= config.height_tiles {
        return Ok("outside");
    }
    let destination = generator
        .tile_at_with_cancel(to, &|| false)?
        .ok_or(EnvironmentPageError::Invalid)?;
    if let Some(object) = generator.object_at_with_cancel(to, &|| false)? {
        return Ok(match object.object {
            ObjectKind::Tree => "tree_object",
            ObjectKind::ForageBush => "forage_bush_object",
            ObjectKind::GoldDeposit => "gold_object",
            ObjectKind::StoneDeposit => "stone_object",
            ObjectKind::Decoration => "decoration_object",
        });
    }
    if destination.water != WaterKind::None {
        return Ok("water");
    }
    let source = generator
        .tile_at_with_cancel(from, &|| false)?
        .ok_or(EnvironmentPageError::Invalid)?;
    if source.surface.kind == SurfaceKind::Cliff
        || destination.surface.kind == SurfaceKind::Cliff
        || (i32::from(source.game_height_level) - i32::from(destination.game_height_level)).abs()
            > 1
        || !terrain.crossable_with_cancel(from, to, config, &|| false)?
    {
        return Ok("cliff_or_grade");
    }
    if !terrain.passable_with_cancel(to, config, &|| false)? {
        return Ok("other_impassable_terrain");
    }
    Ok("other_edge_rule")
}

fn worldcover_status(package: &MapPackage) -> String {
    let locked_sources = package
        .source_locks
        .iter()
        .filter(|source| source.provider.to_ascii_lowercase().contains("worldcover"))
        .count();
    if locked_sources == 0 {
        "unavailable_no_source_lock".to_owned()
    } else {
        format!("unavailable_no_prepared_page;source_locks={locked_sources}")
    }
}
