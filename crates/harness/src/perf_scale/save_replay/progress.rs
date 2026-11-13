//! Finite-window movement observations, sampled outside the instruction span.
use aoe_simulation::GameWorld;
use serde_json::{Value, json};
use std::collections::BTreeMap;

type Tile = (i32, i32);
struct Unit {
    tile: Tile,
    target: Tile,
    moved: bool,
    arrived: bool,
    active: bool,
    seen: u64,
}

#[derive(Default)]
pub(super) struct Progress(BTreeMap<u32, Unit>);

impl Progress {
    pub(super) fn sample(&mut self, world: &GameWorld) {
        for unit in world.units() {
            let tile = unit.position.tile_floor();
            let target = world.movement_order(unit.id).map(|order| {
                let tile = order.destination.tile_floor();
                (tile.x, tile.y)
            });
            self.observe(unit.id.0, (tile.x, tile.y), target, world.tick().0);
        }
    }

    pub(super) fn observe(&mut self, id: u32, tile: Tile, target: Option<Tile>, tick: u64) {
        if let Some(unit) = self.0.get_mut(&id) {
            unit.moved |= unit.tile != tile;
            // Also observe completion when movement removed the order this tick.
            unit.arrived |= tile == unit.target || target == Some(tile);
            unit.tile = tile;
            if let Some(target) = target {
                unit.target = target;
            }
            unit.active = target.is_some();
            unit.seen = tick;
        } else if let Some(target) = target {
            self.0.insert(
                id,
                Unit {
                    tile,
                    target,
                    moved: false,
                    arrived: tile == target,
                    active: true,
                    seen: tick,
                },
            );
        }
    }

    pub(super) fn summary(&self, final_tick: u64) -> Value {
        let count = |predicate: fn(&Unit) -> bool| self.0.values().filter(|u| predicate(u)).count();
        json!({
            "ordered_units": self.0.len(),
            "arrived_units": count(|u| u.arrived),
            "never_arrived_units": count(|u| !u.arrived),
            "zero_tile_movement_units": count(|u| !u.moved),
            "active_at_end_units": self.0.values().filter(|u| u.seen == final_tick && u.active).count(),
            "removed_units": self.0.values().filter(|u| u.seen != final_tick).count(),
            "limits": "Units observed with a movement order at tick boundaries, including the initial state. Arrival means reaching an observed destination tile at least once; motion means changing tile after first observation. Ongoing, replaced, failed and removed orders can be in never-arrived; these counts alone do not prove a unit is stuck. Sub-tile motion and orders issued and completed within one tick are not observed."
        })
    }
}
