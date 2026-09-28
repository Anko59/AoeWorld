use super::super::navigation_report::{BoundedConnectivityDiagnostic, ConnectivityProbeOutcome};
use aoe_core::{TileCoord, WorldConfig};
use aoe_map::EnvironmentPageError;
use aoe_simulation::Terrain;
use std::collections::{BTreeSet, VecDeque};

const NORTH_CORRIDOR_SCAN_RADIUS_TILES: i32 = 16;

pub(super) fn bounded_connectivity_diagnostic(
    terrain: &Terrain,
    config: WorldConfig,
    origin: TileCoord,
    destination: TileCoord,
) -> Result<BoundedConnectivityDiagnostic, EnvironmentPageError> {
    let node_limit = aoe_map::MAX_ROUTE_PLANNER_NODES;
    let mut visited = BTreeSet::from([origin]);
    let mut pending = VecDeque::from([origin]);
    let mut outcome = ConnectivityProbeOutcome::NodeLimit;
    'search: while let Some(tile) = pending.pop_front() {
        if tile == destination {
            outcome = ConnectivityProbeOutcome::Connected;
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
                if visited.len() >= node_limit {
                    break 'search;
                }
                visited.insert(next);
                pending.push_back(next);
            }
        }
    }
    if outcome == ConnectivityProbeOutcome::NodeLimit && pending.is_empty() {
        outcome = ConnectivityProbeOutcome::Disconnected;
    }
    let mut tested_parallel_corridor_count = 0;
    let mut connected_parallel_corridor_x_offset = None;
    if origin.x == destination.x {
        'lanes: for magnitude in 0..=NORTH_CORRIDOR_SCAN_RADIUS_TILES {
            let offsets = if magnitude == 0 {
                [0, 0]
            } else {
                [magnitude, -magnitude]
            };
            for offset in offsets.into_iter().take(if magnitude == 0 { 1 } else { 2 }) {
                tested_parallel_corridor_count += 1;
                if !horizontal_connector_is_clear(terrain, config, origin, offset)?
                    || !horizontal_connector_is_clear(terrain, config, destination, offset)?
                {
                    continue;
                }
                let x = origin.x + offset;
                let mut previous = TileCoord::new(x, origin.y);
                let mut clear = true;
                let step = (destination.y - origin.y).signum();
                for _ in 0..origin.y.abs_diff(destination.y) {
                    let next = TileCoord::new(previous.x, previous.y + step);
                    if !terrain.passable_with_cancel(next, config, &|| false)?
                        || !terrain.crossable_with_cancel(previous, next, config, &|| false)?
                    {
                        clear = false;
                        break;
                    }
                    previous = next;
                }
                if clear {
                    connected_parallel_corridor_x_offset = Some(offset);
                    outcome = ConnectivityProbeOutcome::Connected;
                    break 'lanes;
                }
            }
        }
    }
    Ok(BoundedConnectivityDiagnostic {
        origin: [origin.x, origin.y],
        destination: [destination.x, destination.y],
        outcome,
        visited_tiles: visited.len(),
        frontier_tiles_at_stop: pending.len(),
        node_limit,
        tested_parallel_corridor_radius_tiles: NORTH_CORRIDOR_SCAN_RADIUS_TILES,
        tested_parallel_corridor_count,
        connected_parallel_corridor_x_offset,
    })
}

fn horizontal_connector_is_clear(
    terrain: &Terrain,
    config: WorldConfig,
    endpoint: TileCoord,
    offset: i32,
) -> Result<bool, EnvironmentPageError> {
    let step = offset.signum();
    let mut previous = endpoint;
    for _ in 0..offset.unsigned_abs() {
        let next = TileCoord::new(previous.x + step, endpoint.y);
        if !terrain.passable_with_cancel(next, config, &|| false)?
            || !terrain.crossable_with_cancel(previous, next, config, &|| false)?
        {
            return Ok(false);
        }
        previous = next;
    }
    Ok(true)
}
