use super::{AnimalKind, AnimalTask, CELL_TILES, StoredAnimal, cell, distance, radius, unit_key};
use crate::{DamageKind, GameWorld, HitRecord, UnitKind, damage_dealt};
use aoe_core::{EntityId, FIXED_SUBUNITS_PER_TILE, WorldPosition};
use std::collections::BTreeMap;

impl GameWorld {
    pub(crate) fn advance_wildlife(&mut self, changed: &mut Vec<EntityId>) {
        self.wildlife.stats = Default::default();
        self.advance_hunters(changed);
        // An animal outside every live unit's neighbourhood needs only its
        // ongoing attack or flee step. Count broad-phase pairs per cell and
        // score individual units only in cells that touch acquisition range.
        let mut nearby: BTreeMap<EntityId, Option<EntityId>> =
            self.wildlife.engaged.iter().map(|id| (*id, None)).collect();
        self.passes.record(
            crate::game_world::Pass::WildlifeIds,
            self.wildlife.animals.len(),
        );
        const CELL_SUBUNITS: i32 = CELL_TILES * FIXED_SUBUNITS_PER_TILE;
        for a in self.wildlife.animals.values().map(|animal| animal.state) {
            // Carcasses remain in the food-search index but never run animal
            // AI; their state belongs to the decay clock.
            if a.hp == 0 {
                continue;
            }
            let (cx, cy) = cell(a.position);
            let wolf = a.kind == AnimalKind::Wolf;
            let acquire = match a.kind {
                AnimalKind::Boar => false,
                AnimalKind::Sheep => a.owner.is_none(),
                AnimalKind::Deer | AnimalKind::Wolf => true,
            };
            let reach = i32::from(a.kind.rules().sight_tiles) * FIXED_SUBUNITS_PER_TILE;
            let sight = i64::from(reach).pow(2);
            let x_cells = a.position.x.saturating_sub(reach).div_euclid(CELL_SUBUNITS)
                ..=a.position.x.saturating_add(reach).div_euclid(CELL_SUBUNITS);
            let y_cells = a.position.y.saturating_sub(reach).div_euclid(CELL_SUBUNITS)
                ..=a.position.y.saturating_add(reach).div_euclid(CELL_SUBUNITS);
            let mut closest: Option<(i64, aoe_core::PlayerId, EntityId)> = None;
            let mut pairs = 0;
            for y in cy - 1..=cy + 1 {
                let score_row = acquire && y_cells.contains(&y);
                for x in cx - 1..=cx + 1 {
                    let Some(cell) = self.wildlife.unit_cells.get(&unit_key((x, y))) else {
                        continue;
                    };
                    pairs += cell.units.len() - cell.dead;
                    if !score_row || !x_cells.contains(&x) {
                        continue;
                    }
                    for unit in &cell.units {
                        if !unit.alive || (wolf && unit.kind != UnitKind::Villager) {
                            continue;
                        }
                        let gap = distance(unit.position, a.position);
                        let score = (gap, unit.player, unit.id);
                        if gap <= sight && closest.is_none_or(|old| score < old) {
                            closest = Some(score);
                        }
                    }
                }
            }
            if pairs != 0 {
                self.wildlife.stats.candidate_pairs += pairs as u32;
                self.wildlife.stats.cells += 9;
                nearby.insert(a.id, closest.map(|(_, _, unit)| unit));
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
                    if nearest.is_none() {
                        a.herd = None;
                    }
                    // Close pressure starts one short run. A bow-range hunter
                    // does not continually push prey beyond its own range.
                    if a.herd.is_none()
                        && let Some(unit) =
                            nearest.filter(|u| distance(a.state.position, u.position) <= radius(2))
                    {
                        let mut dx = i64::from(a.state.position.x) - i64::from(unit.position.x);
                        let dy = i64::from(a.state.position.y) - i64::from(unit.position.y);
                        if dx == 0 && dy == 0 {
                            dx = 1;
                        }
                        let length = i64::from(crate::game_path::segment_length(dx, dy));
                        let stride = 2 * i64::from(FIXED_SUBUNITS_PER_TILE);
                        a.herd = Some(WorldPosition::new(
                            (i64::from(a.state.position.x) + dx * stride / length) as i32,
                            (i64::from(a.state.position.y) + dy * stride / length) as i32,
                        ));
                    }
                    if let Some(to) = a.herd {
                        let from = a.state.position;
                        self.move_animal(
                            &mut a,
                            i64::from(to.x) - i64::from(from.x),
                            i64::from(to.y) - i64::from(from.y),
                        );
                        a.state.task = AnimalTask::Fleeing;
                        if a.state.position == to || !a.state.moving {
                            a.herd = None;
                        }
                    }
                }
                AnimalKind::Wolf => {
                    if a.state
                        .target
                        .and_then(|id| self.unit(id))
                        .is_none_or(|u| u.hp == 0 || self.embarked(u.id))
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
                && !self.embarked(u.id)
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
            a.state.task = AnimalTask::Idle;
            self.move_predator(a, target.position);
            return;
        }
        a.herd = None;
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
        let (victim, killed, at) = (unit.player, unit.hp == 0, unit.position.tile_floor());
        self.combat.healing.notify(at);
        self.record_combat_hit(None, victim, false, killed);
        if killed {
            self.kill_unit(index);
        }
        changed.push(self.units[index].state.id);
        a.cooldown = (self.config.tick_hz * rules.attack_period_centiseconds / 100).max(1);
    }

    pub(super) fn move_animal(&self, a: &mut StoredAnimal, dx: i64, dy: i64) {
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
