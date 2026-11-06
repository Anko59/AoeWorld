//! Save and resume: the part of a running game that people care about, as a
//! plain value, and the way back into a freshly built world of the same map.
//!
//! A snapshot keeps the tick, every player's economy, every building (progress,
//! training queue, research, rally point, garrison), every living unit (place,
//! health, load, stance) with the job it was doing, and the map's resource
//! changes. Restoring rebuilds the world through the ordinary placement and
//! order paths, so every derived index is built by the code that owns it and
//! entity identities (which AI processes hold) are preserved.
//!
//! Kept beyond that: Steward, General and Vassal settings, army groups and
//! their waypoints, queued moves, hunts, animals and fog exploration. Not
//! kept, and restarted instead: in-flight route searches, fog memory of
//! destroyed buildings, carcasses, and what a General was deciding.
//! `RestoreReport` counts what a restore could not place.
mod records;
use super::{ConstructionState, GameWorld, GameWorldError};
use aoe_core::EntityId;
pub use records::*;
use std::collections::BTreeMap;

impl GameWorld {
    /// The resumable state of this game; `content_hash` binds the resource
    /// changes to the map package.
    pub fn snapshot(&self, content_hash: [u8; 32]) -> Option<WorldSnapshot> {
        let resources = self.resource_snapshot(content_hash)?;
        let assignments: BTreeMap<EntityId, (u64, Option<EntityId>)> = self
            .economy
            .gatherers
            .assignments()
            .into_iter()
            .map(|(id, node, farm)| (id, (node, farm)))
            .collect();
        let mut units: Vec<UnitRecord> = self
            .units
            .iter()
            .filter(|stored| stored.state.hp > 0 && stored.state.death_tick.is_none())
            .map(|stored| UnitRecord {
                unit: stored.state,
                next_hit_tick: stored.next_hit_tick,
                gather: assignments.get(&stored.state.id).copied(),
                build: self
                    .economy
                    .builders
                    .get(&stored.state.id)
                    .map(|task| task.building),
                garrison_in: None,
                move_to: stored
                    .formation
                    .is_none()
                    .then_some(stored.order)
                    .flatten()
                    .map(|o| o.destination),
                queue: self
                    .move_queues
                    .get(&stored.state.id)
                    .map_or_else(Vec::new, |q| q.iter().copied().collect()),
                hunt: self.hunting_target(stored.state.id),
                management: self.management_of(stored.state.id, stored.state.kind),
            })
            .collect();
        for building in self.economy.buildings.values() {
            for held in &building.garrison {
                units.push(UnitRecord {
                    unit: held.unit,
                    next_hit_tick: held.next_hit_tick,
                    gather: None,
                    build: None,
                    garrison_in: Some(building.id),
                    move_to: None,
                    queue: Vec::new(),
                    hunt: None,
                    management: self.management_of(held.unit.id, held.unit.kind),
                });
            }
        }
        units.sort_by_key(|record| record.unit.id);
        let buildings = self
            .economy
            .buildings
            .values()
            .map(|b| BuildingRecord {
                id: b.id,
                player: b.player,
                kind: b.kind,
                footprint: b.footprint,
                age: b.age,
                complete: b.state == ConstructionState::Complete,
                bridge_span: b.bridge_span,
                training_queue: b.training_queue.iter().copied().collect(),
                last_hit: b.last_hit,
                work_done: b.work_done,
                work_total: b.work_total,
                hp: b.hp,
                max_hp: b.max_hp,
                los: b.los,
                food_remaining: b.food_remaining,
                research: b.research,
                gate_open: b.gate_open,
                rally_point: b.rally_point,
                farm_reseeds: b.farm_reseeds,
            })
            .collect();
        Some(WorldSnapshot {
            version: WORLD_SNAPSHOT_VERSION,
            tick: self.tick.0,
            next_entity_id: self.next_entity_id,
            fog_disabled: self.fog_disabled(),
            hp_quarters: self.combat.hp_quarters.clone(),
            players: self.economy.players.values().copied().collect(),
            resources,
            buildings,
            units,
            formations: self.formation_records(),
            animals: self.animal_records(),
            explored: self.explored_records(),
        })
    }

    /// Rebuilds `snapshot` into this world: freshly built from the same map,
    /// with its limits set, and holding no units or buildings yet.
    pub fn restore_snapshot(
        &mut self,
        snapshot: &WorldSnapshot,
        content_hash: [u8; 32],
        cancelled: &dyn Fn() -> bool,
    ) -> Result<RestoreReport, SnapshotError> {
        if snapshot.version != WORLD_SNAPSHOT_VERSION {
            return Err(SnapshotError::Version);
        }
        if !self.units.is_empty() || !self.economy.buildings.is_empty() {
            return Err(SnapshotError::NotEmpty);
        }
        self.restore_resources(&snapshot.resources, content_hash, cancelled)
            .map_err(|error| SnapshotError::Map(error.to_string()))?;
        self.set_fog_disabled(snapshot.fog_disabled);
        for player in &snapshot.players {
            self.economy.players.insert(player.player, *player);
        }
        self.tick = aoe_core::Tick(snapshot.tick);
        self.restore_explored(&snapshot.explored);
        let mut report = RestoreReport::default();
        // Identities are placed one at a time at their own number: buildings
        // first (a sheltered unit needs its building), then units.
        for record in &snapshot.buildings {
            self.expect_id(record.id);
            self.restore_building(record, &mut report);
        }
        for record in &snapshot.units {
            self.expect_id(record.unit.id);
            self.restore_unit(record, &mut report);
        }
        for record in &snapshot.animals {
            self.expect_id(record.id);
            if self.restore_animal(record) {
                report.animals += 1;
            } else {
                report.animals_lost += 1;
            }
        }
        self.next_entity_id = snapshot.next_entity_id;
        self.combat.hp_quarters.clone_from(&snapshot.hp_quarters);
        self.resume_jobs(snapshot, &mut report);
        for player in &snapshot.players {
            if let Some(slot) = self.economy.players.get_mut(&player.player) {
                *slot = *player;
            }
        }
        self.remember_visible_buildings();
        Ok(report)
    }

    /// The next allocation hands out `id`.
    fn expect_id(&mut self, id: EntityId) {
        self.next_entity_id = u64::from(id.0);
        self.combat.hp_quarters.resize(id.0 as usize, 0);
    }

    fn restore_building(&mut self, record: &BuildingRecord, report: &mut RestoreReport) {
        let placed = self.insert_building_shape(
            record.player,
            record.kind,
            record.footprint,
            record.complete,
            record.bridge_span,
        );
        let Ok(id) = placed else {
            report.buildings_lost += 1;
            return;
        };
        if id != record.id {
            report.buildings_lost += 1;
            return;
        }
        let Some(building) = self.economy.buildings.get_mut(&id) else {
            return;
        };
        building.age = record.age;
        building.training_queue = record.training_queue.iter().copied().collect();
        building.last_hit = record.last_hit;
        building.work_done = record.work_done;
        building.work_total = record.work_total;
        building.hp = record.hp;
        building.max_hp = record.max_hp;
        building.los = record.los;
        building.food_remaining = record.food_remaining;
        building.research = record.research;
        building.gate_open = record.gate_open;
        building.rally_point = record.rally_point;
        building.farm_reseeds = record.farm_reseeds;
        if !record.training_queue.is_empty() {
            self.economy.active_training.insert(id);
        }
        if record.research.is_some() {
            self.economy.active_research.insert(id);
        }
        if record.gate_open {
            self.economy.open_gates.insert(id);
        }
        report.buildings += 1;
    }

    fn restore_unit(&mut self, record: &UnitRecord, report: &mut RestoreReport) {
        let state = record.unit;
        let spawned = self.spawn_kind(state.player, state.kind, state.position);
        // A sheltered unit's saved place may lie inside its building: it
        // appears beside it and is sheltered at once.
        let spawned = match (spawned, record.garrison_in.and_then(|id| self.building(id))) {
            (Err(_), Some(building)) => {
                let rect = building.footprint;
                let ring: Vec<aoe_core::TileCoord> = (rect.min.x - 1..=rect.max.x)
                    .flat_map(|x| (rect.min.y - 1..=rect.max.y).map(move |y| (x, y)))
                    .filter(|(x, y)| !rect.contains(aoe_core::TileCoord::new(*x, *y)))
                    .map(|(x, y)| aoe_core::TileCoord::new(x, y))
                    .collect();
                let mut placed = Err(GameWorldError::InvalidPosition);
                for tile in ring {
                    let Ok(at) = aoe_core::WorldPosition::from_tile_center(tile) else {
                        continue;
                    };
                    placed = self.spawn_kind(state.player, state.kind, at);
                    if placed.is_ok() {
                        break;
                    }
                    self.next_entity_id = u64::from(state.id.0);
                }
                placed
            }
            (result, _) => result,
        };
        let Ok(id) = spawned else {
            report.units_lost += 1;
            return;
        };
        if id != state.id {
            report.units_lost += 1;
            return;
        }
        let slot = self.lookup[&id];
        let stored = &mut self.units[slot];
        stored.next_hit_tick = record.next_hit_tick;
        let unit = &mut stored.state;
        unit.hp = state.hp;
        unit.max_hp = state.max_hp;
        unit.facing = state.facing;
        unit.carried = state.carried;
        unit.stance = state.stance;
        unit.auto_scout = state.auto_scout;
        unit.sergeant = state.sergeant;
        unit.last_hit = state.last_hit;
        if let Some(building) = record.garrison_in {
            self.shelter_restored(id, building);
        }
        report.units += 1;
    }

    /// What a management unit was set to, if it is one.
    fn management_of(&self, id: EntityId, kind: crate::UnitKind) -> Option<Management> {
        match kind {
            crate::UnitKind::Steward => self.steward_settings(id).map(Management::Steward),
            crate::UnitKind::Vassal => self.vassal_status(id).map(|status| Management::Vassal {
                development: status.development,
            }),
            crate::UnitKind::General => {
                self.economy
                    .generals
                    .armies
                    .get(&id)
                    .map(|army| Management::General {
                        training: army.training,
                        mix: army.mix,
                        solo: army.solo,
                        destination: army.destination,
                    })
            }
            _ => None,
        }
    }

    /// Puts management settings back; the unit's own activation (sheltering in
    /// its Town Center or Castle) has already happened.
    fn restore_management(&mut self, record: &UnitRecord) -> bool {
        let (id, player) = (record.unit.id, record.unit.player);
        match record.management {
            Some(Management::Steward(settings)) => {
                self.issue_steward_settings(player, id, settings).is_ok()
            }
            Some(Management::Vassal { development }) => self
                .issue_vassal_development(player, id, development)
                .is_ok(),
            Some(Management::General {
                training,
                mix,
                solo,
                destination,
            }) => {
                let Some(army) = self.economy.generals.armies.get_mut(&id) else {
                    return false;
                };
                army.training = training;
                army.mix = mix;
                army.solo = solo;
                if let Some(destination) = destination {
                    army.destination = Some(destination);
                    army.state = crate::GeneralState::Marching;
                    army.routed = None;
                }
                true
            }
            None => false,
        }
    }

    /// Orders units back to what they were doing, through the ordinary order
    /// paths; a refused order leaves the unit idle.
    fn resume_jobs(&mut self, snapshot: &WorldSnapshot, report: &mut RestoreReport) {
        let mut grouped = std::collections::BTreeSet::new();
        for group in &snapshot.formations {
            let members: Vec<EntityId> = group
                .members
                .iter()
                .copied()
                .filter(|id| {
                    self.lookup.contains_key(id) && self.unit(*id).is_some_and(|u| u.hp > 0)
                })
                .collect();
            let Some(player) = members
                .first()
                .and_then(|id| self.unit(*id))
                .map(|u| u.player)
            else {
                continue;
            };
            let moved = self
                .issue_formation_move(
                    player,
                    &members,
                    group.form,
                    group.destination,
                    false,
                    group.attack_move,
                )
                .is_ok();
            for (at, _) in &group.queue {
                let _ = self.issue_formation_move(
                    player,
                    &members,
                    group.form,
                    *at,
                    true,
                    group.attack_move,
                );
            }
            if moved {
                grouped.extend(members);
                report.formations += 1;
            }
        }
        for record in &snapshot.units {
            let (id, player) = (record.unit.id, record.unit.player);
            if record.management.is_some() && self.restore_management(record) {
                report.managers += 1;
            }
            if record.garrison_in.is_some() {
                continue;
            }
            let result = if let Some((_, Some(farm))) = record.gather {
                self.issue_farm(player, id, farm)
            } else if let Some((node, None)) = record.gather {
                self.issue_gather(player, id, node)
            } else if let Some(building) = record.build {
                self.issue_construct(player, &[id], building)
            } else if let Some(animal) = record.hunt {
                self.issue_hunt(player, &[id], animal)
            } else if let Some(target) = record.unit.attack_target {
                self.issue_attack(player, id, target)
            } else if let Some(at) = record.move_to.filter(|_| !grouped.contains(&id)) {
                let result = self
                    .issue_move(id, at)
                    .map(|_| ())
                    .map_err(crate::EconomyError::Movement);
                for waypoint in &record.queue {
                    let _ = self.issue_queued_move(id, *waypoint);
                }
                result
            } else {
                continue;
            };
            match result {
                Ok(()) => report.jobs_resumed += 1,
                Err(_) => report.jobs_lost += 1,
            }
        }
    }
}
