use super::{AnimalKind, AnimalTask, StoredAnimal, cell, distance, radius, unit_key};
use crate::{DamageKind, GameWorld, HitRecord, UnitKind, damage_dealt};
use aoe_core::{EntityId, FIXED_SUBUNITS_PER_TILE, WorldPosition};
use std::collections::BTreeMap;

impl GameWorld {
    pub(crate) fn advance_wildlife(&mut self, changed: &mut Vec<EntityId>) {
        self.wildlife.stats = Default::default();
        self.advance_hunters(changed);
        // Animal-driven: each animal probes the unit cell grid around itself.
        // An animal no unit can reach through its nine-cell neighbourhood is
        // never visited, so idle populations cost nothing.
        let mut nearby: BTreeMap<EntityId, Option<EntityId>> =
            self.wildlife.engaged.iter().map(|id| (*id, None)).collect();
        let ids = self.wildlife.animals.keys().copied().collect::<Vec<_>>();
        for id in ids {
            let a = self.wildlife.animals[&id].state;
            // Carcasses remain in the food-search index but never run animal
            // AI; their state belongs to the decay clock.
            if a.hp == 0 {
                continue;
            }
            let (cx, cy) = cell(a.position);
            let wolf = a.kind == AnimalKind::Wolf;
            let sight = radius(i64::from(a.kind.rules().sight_tiles));
            let mut closest: Option<(i64, aoe_core::PlayerId, EntityId)> = None;
            let mut probed = false;
            for y in cy - 1..=cy + 1 {
                for x in cx - 1..=cx + 1 {
                    let Some(units) = self.wildlife.unit_cells.get(&unit_key((x, y))) else {
                        continue;
                    };
                    for unit in units {
                        if !unit.alive {
                            continue;
                        }
                        probed = true;
                        self.wildlife.stats.candidate_pairs += 1;
                        let gap = distance(unit.position, a.position);
                        let score = (gap, unit.player, unit.id);
                        if (!wolf || unit.kind == UnitKind::Villager)
                            && gap <= sight
                            && closest.is_none_or(|old| score < old)
                        {
                            closest = Some(score);
                        }
                    }
                }
            }
            if probed {
                self.wildlife.stats.cells += 9;
                nearby.insert(id, closest.map(|(_, _, unit)| unit));
            }
        }
        for (id, nearest) in nearby {
            let Some(mut a) = self.wildlife.animals.get(&id).copied() else {
                continue;
            };
            self.wildlife.stats.animal_steps += 1;
            a.state.moving = false;
            let old_cell = cell(a.state.position);
            let nearest = nearest.and_then(|u| self.unit(u)).filter(|u| u.hp > 0);
            match a.state.kind {
                AnimalKind::Sheep => {
                    if a.state.owner.is_none() {
                        a.state.owner = nearest.map(|u| u.player);
                    }
                    if let Some(destination) = a.herd {
                        let from = a.state.position;
                        if from != destination {
                            self.move_animal(
                                &mut a,
                                i64::from(destination.x) - i64::from(from.x),
                                i64::from(destination.y) - i64::from(from.y),
                            );
                        }
                        if a.state.position == destination || !a.state.moving {
                            a.herd = None;
                            a.state.moving = false;
                        }
                    }
                }
                AnimalKind::Deer => {
                    a.state.task = AnimalTask::Idle;
                    if let Some(unit) = nearest {
                        a.state.task = AnimalTask::Fleeing;
                        let dx = i64::from(a.state.position.x) - i64::from(unit.position.x);
                        let dy = i64::from(a.state.position.y) - i64::from(unit.position.y);
                        self.move_animal(&mut a, if dx == 0 && dy == 0 { 1 } else { dx }, dy);
                    }
                }
                AnimalKind::Wolf => {
                    if a.state
                        .target
                        .and_then(|id| self.unit(id))
                        .is_none_or(|u| u.hp == 0)
                    {
                        a.state.target = nearest.map(|u| u.id);
                    }
                    self.animal_attack(&mut a, changed);
                }
                AnimalKind::Boar => self.animal_attack(&mut a, changed),
            }
            let new_cell = cell(a.state.position);
            if a.state.target.is_some() || a.state.task == AnimalTask::Fleeing || a.herd.is_some() {
                self.wildlife.engaged.insert(id);
            } else {
                self.wildlife.engaged.remove(&id);
            }
            if old_cell != new_cell {
                self.unindex_animal(id, old_cell);
                self.wildlife.cells.entry(new_cell).or_default().insert(id);
            }
            self.wildlife.animals.insert(id, a);
            self.update_observer(
                id,
                a.state.owner.filter(|_| a.state.hp > 0).map(|p| {
                    (
                        p,
                        a.state.position.tile_floor(),
                        a.state.kind.rules().los_tiles,
                    )
                }),
            );
        }
        self.advance_carcasses();
    }

    fn animal_attack(&mut self, a: &mut StoredAnimal, changed: &mut Vec<EntityId>) {
        let rules = a.state.kind.rules();
        a.cooldown = a.cooldown.saturating_sub(1);
        let target = a.state.target.and_then(|id| self.unit(id)).filter(|u| {
            u.hp > 0
                && (rules.chase_tiles == 0
                    || distance(u.position, a.home) <= radius(i64::from(rules.chase_tiles)))
        });
        let Some(target) = target else {
            a.state.target = None;
            a.state.task = AnimalTask::Idle;
            return;
        };
        a.state.task = AnimalTask::Attacking;
        a.state.facing = crate::game_world::facing_for(
            i64::from(target.position.x) - i64::from(a.state.position.x),
            i64::from(target.position.y) - i64::from(a.state.position.y),
            a.state.facing,
        );
        if distance(a.state.position, target.position) > radius(1) {
            self.move_animal(
                a,
                i64::from(target.position.x) - i64::from(a.state.position.x),
                i64::from(target.position.y) - i64::from(a.state.position.y),
            );
            return;
        }
        if a.cooldown > 0 {
            return;
        }
        let Some(&index) = self.lookup.get(&target.id) else {
            return;
        };
        let (melee, pierce) = self.unit_armour(target.id).unwrap_or_default();
        let unit = &mut self.units[index].state;
        unit.hp =
            unit.hp
                .saturating_sub(damage_dealt(rules.attack, DamageKind::Melee, melee, pierce));
        unit.last_hit = Some(HitRecord {
            attacker: a.state.id,
            tick: self.tick,
        });
        if unit.hp == 0 {
            self.kill_unit(index);
        }
        changed.push(self.units[index].state.id);
        a.cooldown = (self.config.tick_hz * rules.attack_period_centiseconds / 100).max(1);
    }

    fn move_animal(&self, a: &mut StoredAnimal, dx: i64, dy: i64) {
        a.state.facing = crate::game_world::facing_for(dx, dy, a.state.facing);
        let numerator =
            u64::from(a.state.kind.rules().speed_hundredths) * FIXED_SUBUNITS_PER_TILE as u64;
        let denominator = 100 * u64::from(self.config.tick_hz);
        a.speed_carry += numerator;
        let step = (a.speed_carry / denominator) as i64;
        a.speed_carry %= denominator;
        let length = i64::from(crate::game_path::segment_length(dx, dy));
        let step = step.min(length);
        let from = a.state.position;
        let to = WorldPosition::new(
            (i64::from(from.x) + dx * step / length) as i32,
            (i64::from(from.y) + dy * step / length) as i32,
        );
        // Small, bounded substeps also handle tick_hz == 1 without tunnelling.
        let count = (step / (i64::from(FIXED_SUBUNITS_PER_TILE) / 4) + 1).max(1);
        for n in 1..=count {
            let next = WorldPosition::new(
                from.x + ((i64::from(to.x - from.x) * n) / count) as i32,
                from.y + ((i64::from(to.y - from.y) * n) / count) as i32,
            );
            let old = a.state.position.tile_floor();
            let tile = next.tile_floor();
            if !self.config.valid_ground_position(next)
                || !self.tile_passable(tile)
                || !self.building_step_clear(old, tile)
                // A map's edge rule blocks a tile against itself: a step
                // inside one tile has no edge to cross.
                || (old != tile && !self.terrain.crossable(old, tile, self.config))
                || (old.x != tile.x
                    && old.y != tile.y
                    && (!self.tile_passable(aoe_core::TileCoord::new(old.x, tile.y))
                        || !self.tile_passable(aoe_core::TileCoord::new(tile.x, old.y))))
            {
                break;
            }
            a.state.position = next;
        }
        a.state.moving = a.state.position != from;
    }

    pub(super) fn unindex_animal(&mut self, id: EntityId, key: (i32, i32)) {
        if let Some(ids) = self.wildlife.cells.get_mut(&key) {
            ids.remove(&id);
            if ids.is_empty() {
                self.wildlife.cells.remove(&key);
            }
        }
    }
}
