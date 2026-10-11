//! Bounded local reachability observation, not an activation or routing verdict.
use super::SourceQualificationError;
use aoe_core::{TileCoord, WorldConfig};
use aoe_simulation::Terrain;
use serde::Serialize;
use std::collections::{BTreeSet, VecDeque};

const MAX_TILES: usize = 4096;
const MAX_WORK: usize = MAX_TILES * 8;

#[derive(Debug, Serialize)]
pub struct ComponentObservation {
    pub policy: &'static str,
    pub visited_tiles: usize,
    pub bounds: Option<[i32; 4]>,
    pub probe_work: usize,
    pub maximum_tiles: usize,
    pub maximum_work: usize,
    pub truncated: bool,
    pub proves_complete_component: bool,
    pub proves_planner_route: bool,
}

pub(super) fn observe(
    terrain: &Terrain,
    config: WorldConfig,
    origin: TileCoord,
) -> Result<ComponentObservation, SourceQualificationError> {
    let mut visited = BTreeSet::new();
    let mut pending = VecDeque::new();
    if terrain.passable_with_cancel(origin, config, &|| false)? {
        visited.insert(origin);
        pending.push_back(origin);
    }
    let mut work = 0;
    let mut truncated = false;
    'search: while let Some(tile) = pending.pop_front() {
        for dy in -1..=1 {
            for dx in -1..=1 {
                if (dx, dy) == (0, 0) {
                    continue;
                }
                if work == MAX_WORK || visited.len() == MAX_TILES {
                    truncated = true;
                    break 'search;
                }
                work += 1;
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
    let bounds = visited.iter().next().map(|first| {
        visited
            .iter()
            .fold([first.x, first.y, first.x, first.y], |bounds, tile| {
                [
                    bounds[0].min(tile.x),
                    bounds[1].min(tile.y),
                    bounds[2].max(tile.x),
                    bounds[3].max(tile.y),
                ]
            })
    });
    Ok(ComponentObservation {
        policy: "country-local-component-4096-tiles-32768-work-v1",
        visited_tiles: visited.len(),
        bounds,
        probe_work: work,
        maximum_tiles: MAX_TILES,
        maximum_work: MAX_WORK,
        truncated,
        proves_complete_component: !truncated && !visited.is_empty(),
        proves_planner_route: false,
    })
}
