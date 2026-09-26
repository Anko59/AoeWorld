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
    boundary_terrain: BoundaryTerrainEvidence,
    forest_pattern: ForestPatternEvidence,
}

#[derive(Debug, Default)]
pub(super) struct BoundaryTerrainEvidence {
    pub(super) cliff_surface_edges: usize,
    pub(super) cliff_edges_supported_by_geographic_rise: usize,
    pub(super) cliff_geographic_rise_sum_cm: u64,
    pub(super) cliff_geographic_rise_max_cm: u32,
    pub(super) grade_discontinuity_edges: usize,
    pub(super) grade_geographic_rise_max_cm: u32,
}

#[derive(Debug, Default)]
pub(super) struct ForestPatternEvidence {
    pub(super) tree_blocker_tiles: usize,
    pub(super) eight_connected_clumps: usize,
    pub(super) largest_clump_tiles: usize,
    pub(super) isolated_tree_tiles: usize,
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
        let (boundary_edges, blocker_tiles, boundary_terrain, forest_pattern) =
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
            boundary_terrain,
            forest_pattern,
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
                        ",biomes={:?},blocked_frontier_edges={:?},distinct_blocker_tiles={:?},geographic_cliff_gradient={:?},forest_pattern={:?},diagnostic_conclusion={}",
                        component.biomes,
                        component.boundary_edges,
                        component.blocker_tiles,
                        component.boundary_terrain,
                        component.forest_pattern,
                        diagnostic_conclusion(
                            &component.boundary_terrain,
                            &component.forest_pattern
                        )
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
    let (edge_counts, blocker_tiles, boundary_terrain, forest_pattern) =
        boundary_counts(terrain, generator, tiles, config)?;
    Ok(format!(
        "blocked_frontier_edges={edge_counts:?},distinct_blocker_tiles={blocker_tiles:?},geographic_cliff_gradient={boundary_terrain:?},forest_pattern={forest_pattern:?},diagnostic_conclusion={}",
        diagnostic_conclusion(&boundary_terrain, &forest_pattern)
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
) -> Result<
    (
        BlockerCounts,
        BlockerCounts,
        BoundaryTerrainEvidence,
        ForestPatternEvidence,
    ),
    EnvironmentPageError,
> {
    let mut edge_counts = BTreeMap::new();
    let mut blocker_cells = BTreeMap::<TileCoord, &'static str>::new();
    let mut boundary_terrain = BoundaryTerrainEvidence::default();
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
                let reason = blocker_reason(
                    terrain,
                    generator,
                    *tile,
                    next,
                    config,
                    &mut boundary_terrain,
                )?;
                *edge_counts.entry(reason).or_insert(0) += 1;
                blocker_cells.entry(next).or_insert(reason);
            }
        }
    }
    let mut tile_counts = BTreeMap::new();
    for reason in blocker_cells.values() {
        *tile_counts.entry(*reason).or_insert(0) += 1;
    }
    let forest_pattern = forest_pattern(&blocker_cells);
    Ok((edge_counts, tile_counts, boundary_terrain, forest_pattern))
}

fn blocker_reason(
    terrain: &Terrain,
    generator: &MapChunkGenerator,
    from: TileCoord,
    to: TileCoord,
    config: WorldConfig,
    boundary_terrain: &mut BoundaryTerrainEvidence,
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
    let geographic_rise_cm = source
        .geographic_height_centimeters
        .abs_diff(destination.geographic_height_centimeters);
    if source.surface.kind == SurfaceKind::Cliff || destination.surface.kind == SurfaceKind::Cliff {
        boundary_terrain.cliff_surface_edges += 1;
        boundary_terrain.cliff_geographic_rise_sum_cm = boundary_terrain
            .cliff_geographic_rise_sum_cm
            .saturating_add(u64::from(geographic_rise_cm));
        boundary_terrain.cliff_geographic_rise_max_cm = boundary_terrain
            .cliff_geographic_rise_max_cm
            .max(geographic_rise_cm);
        boundary_terrain.cliff_edges_supported_by_geographic_rise +=
            usize::from(geographic_rise_cm >= aoe_map::ELEVATION_LEVEL_CENTIMETERS as u32);
        return Ok("cliff_surface");
    }
    let game_level_rise =
        (i32::from(source.game_height_level) - i32::from(destination.game_height_level)).abs();
    if game_level_rise > 1 {
        boundary_terrain.grade_discontinuity_edges += 1;
        boundary_terrain.grade_geographic_rise_max_cm = boundary_terrain
            .grade_geographic_rise_max_cm
            .max(geographic_rise_cm);
        return Ok("grade_discontinuity");
    }
    if !terrain.crossable_with_cancel(from, to, config, &|| false)? {
        return Ok("other_edge_rule");
    }
    if !terrain.passable_with_cancel(to, config, &|| false)? {
        return Ok("other_impassable_terrain");
    }
    Ok("other_edge_rule")
}

pub(super) fn forest_pattern(
    blocker_cells: &BTreeMap<TileCoord, &'static str>,
) -> ForestPatternEvidence {
    let mut remaining = blocker_cells
        .iter()
        .filter_map(|(tile, reason)| (*reason == "tree_object").then_some(*tile))
        .collect::<BTreeSet<_>>();
    let tree_blocker_tiles = remaining.len();
    let mut evidence = ForestPatternEvidence {
        tree_blocker_tiles,
        ..ForestPatternEvidence::default()
    };
    while let Some(root) = remaining.pop_first() {
        evidence.eight_connected_clumps += 1;
        let mut pending = VecDeque::from([root]);
        let mut clump_tiles = 1_usize;
        while let Some(tile) = pending.pop_front() {
            for dy in -1..=1 {
                for dx in -1..=1 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let next = TileCoord::new(tile.x + dx, tile.y + dy);
                    if remaining.remove(&next) {
                        clump_tiles += 1;
                        pending.push_back(next);
                    }
                }
            }
        }
        evidence.largest_clump_tiles = evidence.largest_clump_tiles.max(clump_tiles);
        evidence.isolated_tree_tiles += usize::from(clump_tiles == 1);
    }
    evidence
}

pub(super) fn diagnostic_conclusion(
    boundary: &BoundaryTerrainEvidence,
    forest: &ForestPatternEvidence,
) -> &'static str {
    if boundary.cliff_surface_edges > 0
        && boundary.cliff_edges_supported_by_geographic_rise * 2 >= boundary.cliff_surface_edges
    {
        "geographic_elevation_supports_most_cliff_frontier_edges"
    } else if boundary.cliff_surface_edges > 0 {
        "most_cliff_frontier_edges_have_sub_meter_source_rise;review_raster_or_quantization_fragmentation"
    } else if forest.tree_blocker_tiles > 0
        && forest.isolated_tree_tiles * 2 > forest.tree_blocker_tiles
    {
        "most_tree_blockers_are_isolated;review_procedural_forest_fragmentation"
    } else if forest.largest_clump_tiles >= 4 {
        "tree_blockers_form_clumps_consistent_with_forest_edges"
    } else {
        "mixed_or_non_geographic_blockers_dominate;inspect_reported_categories"
    }
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
