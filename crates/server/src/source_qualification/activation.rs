use super::{SourceQualificationError, diagnostics::component_blocker_diagnostic};
use aoe_core::{TileCoord, WorldConfig};
use aoe_map::MapChunkGenerator;
use aoe_simulation::Terrain;
use std::collections::{BTreeSet, VecDeque};

pub(super) const ACTIVATION_COMPONENT_POLICY: &str = "ordinary-bounded-component-diagnostic-v1";
const MAX_ACTIVATION_COMPONENT_TILES: usize = 65_536;
const MAX_ACTIVATION_COMPONENT_PROBE_WORK: usize = 524_288;

pub(super) fn ordinary_activation_component_diagnostic(
    terrain: &Terrain,
    generator: &MapChunkGenerator,
    start: TileCoord,
    config: WorldConfig,
) -> Result<String, SourceQualificationError> {
    let component = enumerate_component(terrain, config, start)?;
    let blockers = component_blocker_diagnostic(terrain, generator, &component.tiles, config)?;
    Ok(format!(
        "policy={ACTIVATION_COMPONENT_POLICY},start=({},{}),component_tiles={},bounds=({},{})..({},{}),truncated={},component_probe_work={}/{MAX_ACTIVATION_COMPONENT_PROBE_WORK},{blockers}",
        start.x,
        start.y,
        component.tiles.len(),
        component.bounds.0,
        component.bounds.2,
        component.bounds.1,
        component.bounds.3,
        component.truncated,
        component.probe_work,
    ))
}

#[derive(Debug)]
struct ComponentEnumeration {
    tiles: BTreeSet<TileCoord>,
    bounds: (i32, i32, i32, i32),
    probe_work: usize,
    truncated: bool,
}

fn enumerate_component(
    terrain: &Terrain,
    config: WorldConfig,
    origin: TileCoord,
) -> Result<ComponentEnumeration, SourceQualificationError> {
    let mut tiles = BTreeSet::from([origin]);
    let mut pending = VecDeque::from([origin]);
    let mut probe_work = 0_usize;
    let mut truncated = false;
    while let Some(tile) = pending.pop_front() {
        if tiles.len() >= MAX_ACTIVATION_COMPONENT_TILES {
            truncated = true;
            break;
        }
        for offset_y in -1..=1 {
            for offset_x in -1..=1 {
                if offset_x == 0 && offset_y == 0 {
                    continue;
                }
                probe_work = probe_work.saturating_add(1);
                if probe_work > MAX_ACTIVATION_COMPONENT_PROBE_WORK {
                    return Err(SourceQualificationError::ActivationLimit {
                        phase: "component_probe_work",
                        observed: probe_work as u64,
                        maximum: MAX_ACTIVATION_COMPONENT_PROBE_WORK as u64,
                        diagnostic: format!(
                            "component_origin=({},{}),visited_tiles={}",
                            origin.x,
                            origin.y,
                            tiles.len()
                        ),
                    });
                }
                let next = TileCoord::new(tile.x + offset_x, tile.y + offset_y);
                if tiles.contains(&next)
                    || !terrain.passable_with_cancel(next, config, &|| false)?
                    || !terrain.crossable_with_cancel(tile, next, config, &|| false)?
                {
                    continue;
                }
                tiles.insert(next);
                pending.push_back(next);
            }
        }
    }
    let min_x = tiles.iter().map(|tile| tile.x).min().unwrap_or(origin.x);
    let max_x = tiles.iter().map(|tile| tile.x).max().unwrap_or(origin.x);
    let min_y = tiles.iter().map(|tile| tile.y).min().unwrap_or(origin.y);
    let max_y = tiles.iter().map(|tile| tile.y).max().unwrap_or(origin.y);
    Ok(ComponentEnumeration {
        tiles,
        bounds: (min_x, max_x, min_y, max_y),
        probe_work,
        truncated,
    })
}

#[cfg(test)]
mod tests;
