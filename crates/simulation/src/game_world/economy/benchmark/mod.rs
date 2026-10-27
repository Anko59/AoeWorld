//! Source-free late-game benchmark construction. Uses the ordinary generated
//! map, gather/return state machine, movement, production and wildlife paths.
use crate::{BuildingKind, EconomyError, GameWorld, Stockpile, Tech, WorldLimits};
use aoe_core::{PlayerId, TileCoord, TileRect, WorldPosition};
use aoe_map::{MapRequest, Ratio, ResourceKind};

mod actors;
mod map;

impl GameWorld {
    /// Fixed CI workload: 100 seats, 8,000 villagers and 6,000 soldiers.
    /// Construction is excluded from measurement; no original assets are read.
    pub fn late_game_workload() -> Result<Self, Box<dyn std::error::Error>> {
        build(100)
    }
}

fn position(tile: TileCoord) -> Result<WorldPosition, EconomyError> {
    Ok(WorldPosition::from_tile_center(tile).map_err(crate::GameWorldError::from)?)
}

fn build(players: u16) -> Result<GameWorld, Box<dyn std::error::Error>> {
    let request = MapRequest {
        center_latitude_e7: 466_000_000,
        center_longitude_e7: 25_000_000,
        requested_side_meters: 16_384 * 60,
        compression: Ratio {
            numerator: 30,
            denominator: 1,
        },
        seed: 41,
        ..MapRequest::default()
    };
    let mut world = map::build(request)?;
    world.set_limits(WorldLimits::for_match(100, 200));
    for seat in 0..players {
        let player = PlayerId(seat);
        let base = TileCoord::new(
            512 + i32::from(seat % 10) * 1_536,
            512 + i32::from(seat / 10) * 1_536,
        );
        // A late-game village has already streamed its surrounding ground.
        // Compose only its local chunks, outside every measured tick.
        if let crate::Terrain::Map { generator, .. } = &world.terrain {
            for cy in (base.y - 64).div_euclid(32)..=(base.y + 63).div_euclid(32) {
                for cx in (base.x - 64).div_euclid(32)..=(base.x + 63).div_euclid(32) {
                    generator.precompose_walk_chunk(cx, cy, &|| false)?;
                }
            }
        }
        let mut primary = None;
        for attempt in 0..100 {
            let start = TileCoord::new(base.x + (attempt % 10) * 40, base.y + (attempt / 10) * 40);
            if let Ok(id) = world.initialize_village(player, start) {
                primary = Some(id);
                break;
            }
        }
        let primary = primary.ok_or_else(|| format!("seat {seat}: no starting village"))?;
        let center = world
            .unit(primary)
            .ok_or(EconomyError::UnknownEntity)?
            .position
            .tile_floor();
        let tc = world
            .buildings()
            .find(|b| b.player == player && b.kind == BuildingKind::TownCenter)
            .ok_or(EconomyError::UnknownEntity)?
            .id;
        let mut production = vec![tc];
        let mut research = None;
        for kind in [BuildingKind::Barracks, BuildingKind::TownCenter] {
            let mut placed = None;
            'search: for dy in -24..=24 {
                for dx in -24..=24 {
                    let origin = TileCoord::new(center.x + dx, center.y + dy);
                    let rules = kind.rules();
                    let rect = TileRect::from_xywh(origin.x, origin.y, rules.side, rules.depth);
                    if world.validate_footprint(rect).is_ok() {
                        placed = Some(world.insert_building(player, kind, rect, true)?);
                        break 'search;
                    }
                }
            }
            let id = placed.ok_or_else(|| format!("seat {seat}: no {kind:?} footprint"))?;
            if kind == BuildingKind::TownCenter {
                research = Some(id);
            } else {
                production.push(id);
            }
        }
        // Scenario seeding gives the mature economy its housing and stock;
        // measured ticks and queued orders use the ordinary production rules.
        for tech in [Tech::FeudalAge, Tech::CastleAge] {
            world.complete_research(player, tech, &mut Vec::new());
        }
        let state = world
            .economy
            .players
            .get_mut(&player)
            .ok_or(EconomyError::NotOwner)?;
        state.population_cap = 200;
        state.stockpile = Stockpile {
            food: 100_000,
            wood: 100_000,
            gold: 100_000,
            stone: 100_000,
        };
        let mut villagers: Vec<_> = world
            .units()
            .filter(|u| u.player == player && u.kind == crate::UnitKind::Villager)
            .map(|u| u.id)
            .collect();
        // Seed on the village's connected ground. Merely passable tiles can
        // be on the other side of a forest or cliff and reject every gather.
        let mut spots = Vec::new();
        let mut seen = std::collections::BTreeSet::from([center]);
        let mut pending = std::collections::VecDeque::from([center]);
        while let Some(tile) = pending.pop_front() {
            spots.push(tile);
            if spots.len() >= 1_024 {
                break;
            }
            for (dx, dy) in [(0, -1), (-1, 0), (1, 0), (0, 1)] {
                let next = TileCoord::new(tile.x + dx, tile.y + dy);
                if next.x.abs_diff(center.x) <= 40
                    && next.y.abs_diff(center.y) <= 40
                    && !seen.contains(&next)
                    && world.tile_passable(next)
                    && world.terrain.crossable(tile, next, world.config())
                {
                    seen.insert(next);
                    pending.push_back(next);
                }
            }
        }
        map::mature_resources(&mut world, center, &spots)?;
        spots.retain(|tile| world.tile_passable(*tile));
        for tile in spots.iter().copied().cycle().take(80 - villagers.len()) {
            villagers.push(world.spawn_villager(player, position(tile)?)?);
        }
        actors::seed(&mut world, player, primary, center, &spots, production[1])
            .map_err(|error| format!("seat {seat}: actor setup: {error}"))?;
        // Placing the seeded tower refreshes housing from actual buildings.
        // Restore this mature fixture's housing after all setup construction.
        world
            .economy
            .players
            .get_mut(&player)
            .ok_or(EconomyError::NotOwner)?
            .population_cap = 200;
        // The combined workload respects eco2 reservations: use all resource
        // kinds in the ordinary starting kit, rather than overbooking the few
        // nearby trees. Keep all 80 real gather/return jobs and full queues.
        let mut nodes = std::collections::BTreeMap::new();
        for node in world
            .starting_resources()
            .into_iter()
            .filter(|n| n.tile.x.abs_diff(center.x) <= 30 && n.tile.y.abs_diff(center.y) <= 30)
        {
            nodes.insert(node.id, node);
        }
        for dy in [-12, -6, 0, 6, 12] {
            for dx in [-12, -6, 0, 6, 12] {
                let at = TileCoord::new(center.x + dx, center.y + dy);
                if let Some(node) = world.nearest_resource(at, ResourceKind::Wood, 12, None) {
                    nodes.insert(node.id, node);
                }
            }
        }
        let nodes: Vec<_> = nodes.into_values().collect();
        if nodes.is_empty() {
            return Err(format!("seat {seat}: no gather targets near {center:?}").into());
        }
        for (index, id) in villagers.into_iter().enumerate() {
            // A mature economy has trips at different points of the carry
            // cycle; identical empty inventories would synchronise all 8k jobs.
            let load = if index % 4 == 0 {
                10
            } else {
                (index % 10) as u16
            };
            let mut assigned = false;
            let mut failure = None;
            for offset in 0..nodes.len() {
                let node = nodes[(index + offset) % nodes.len()];
                if load > 0 {
                    let slot = world.lookup[&id];
                    world.units[slot].state.carried = Some(crate::CarriedResource {
                        kind: node.kind,
                        amount: load,
                    });
                }
                match world.issue_gather(player, id, node.id) {
                    Ok(()) => {
                        assigned = true;
                        break;
                    }
                    Err(error) => failure = Some(error),
                }
            }
            if !assigned {
                let free_slots = nodes
                    .iter()
                    .filter(|n| world.node_has_work_slot(n.id, n.kind, id))
                    .count();
                let free_spots = nodes
                    .iter()
                    .filter(|n| world.resource_has_work_spot(id, n.tile))
                    .count();
                return Err(format!("seat {seat}, villager {index} ({id:?}): all {} gather targets refused: {failure:?}; {free_slots} free targets, {free_spots} with work tiles; {} active planners, {} planning budget", nodes.len(), world.active_planner_count, world.planning_budget).into());
            }
        }
        for (index, building) in production.into_iter().enumerate() {
            let kind = if index == 0 {
                crate::UnitKind::Villager
            } else {
                crate::UnitKind::Militia
            };
            for _ in 0..5 {
                world.issue_train(player, building, kind)?;
            }
            // Spread completion events across the measured window rather
            // than making all hundred economies finish on the same tick.
            let entry = world
                .economy
                .buildings
                .get_mut(&building)
                .and_then(|b| b.training_queue.front_mut())
                .ok_or(EconomyError::UnknownEntity)?;
            entry.progress_ticks = if index == 1 && seat.is_multiple_of(4) {
                // A completed queue really retries its occupied exits each tick.
                entry.total_ticks
            } else {
                u64::from(seat) * entry.total_ticks.saturating_sub(385).min(99) / 99
            };
        }
        let research = research.ok_or(EconomyError::UnknownEntity)?;
        world.issue_research(player, research, Tech::Loom)?;
        let entry = world
            .economy
            .buildings
            .get_mut(&research)
            .and_then(|b| b.research.as_mut())
            .ok_or(EconomyError::UnknownEntity)?;
        entry.progress_ticks = u64::from(seat);
    }
    Ok(world)
}

#[cfg(test)]
mod tests;
