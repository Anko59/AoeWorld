//! Bounded local animal navigation over the same collision edges as units.
use super::StoredAnimal;
use crate::GameWorld;
use aoe_core::{TileCoord, WorldPosition};
use std::collections::{BTreeSet, VecDeque};

impl GameWorld {
    fn animal_edge_open(&self, from: TileCoord, to: TileCoord) -> bool {
        self.tile_passable(to)
            && self.building_step_clear(from, to)
            && self.terrain.crossable(from, to, self.config)
    }

    /// A legal tile in a sealed forest pocket is not a legal wildlife start.
    /// At most the 81 tiles of the four-tile neighbourhood are inspected.
    pub(super) fn animal_spawn_open(&self, origin: TileCoord) -> bool {
        let mut seen = BTreeSet::from([origin]);
        let mut queue = VecDeque::from([origin]);
        while let Some(at) = queue.pop_front() {
            if at.x.abs_diff(origin.x).max(at.y.abs_diff(origin.y)) >= 4 {
                return true;
            }
            for (dx, dy) in [(0, -1), (-1, 0), (1, 0), (0, 1)] {
                let next = TileCoord::new(at.x + dx, at.y + dy);
                if !seen.contains(&next) && self.animal_edge_open(at, next) {
                    seen.insert(next);
                    queue.push_back(next);
                }
            }
        }
        false
    }

    /// Certify the whole bounded kit neighbourhood from the village's legal
    /// scout start, so a spacious but disconnected forest pocket is excluded.
    pub(super) fn animal_start_region(
        &self,
        center: TileCoord,
        root: TileCoord,
    ) -> BTreeSet<TileCoord> {
        let mut seen = BTreeSet::from([root]);
        let mut queue = VecDeque::from([root]);
        while let Some(at) = queue.pop_front() {
            for (dx, dy) in [(0, -1), (-1, 0), (1, 0), (0, 1)] {
                let next = TileCoord::new(at.x + dx, at.y + dy);
                if next.x.abs_diff(center.x).max(next.y.abs_diff(center.y)) <= 24
                    && !seen.contains(&next)
                    && self.animal_edge_open(at, next)
                {
                    seen.insert(next);
                    queue.push_back(next);
                }
            }
        }
        seen
    }

    pub(super) fn move_predator(&self, animal: &mut StoredAnimal, target: WorldPosition) {
        let waypoint = animal.herd.unwrap_or(target);
        let from = animal.state.position;
        self.move_animal(
            animal,
            i64::from(waypoint.x) - i64::from(from.x),
            i64::from(waypoint.y) - i64::from(from.y),
        );
        if animal.state.position == waypoint {
            animal.herd = None;
        }
        if animal.state.moving {
            return;
        }
        animal.herd = None;
        // A retained one-tile waypoint avoids searching every sub-tile step.
        // An obstructed chase gets at most 256 local cell expansions.
        let origin = from.tile_floor();
        let goal = target.tile_floor();
        let mut seen = BTreeSet::from([origin]);
        let mut queue = VecDeque::from([(origin, origin)]);
        for _ in 0..256 {
            let Some((at, first)) = queue.pop_front() else {
                break;
            };
            if at == goal {
                animal.herd = WorldPosition::from_tile_center(first).ok();
                break;
            }
            for (dx, dy) in [(0, -1), (-1, 0), (1, 0), (0, 1)] {
                let next = TileCoord::new(at.x + dx, at.y + dy);
                if !seen.contains(&next) && self.animal_edge_open(at, next) {
                    seen.insert(next);
                    queue.push_back((next, if at == origin { next } else { first }));
                }
            }
        }
    }
}
