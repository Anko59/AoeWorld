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
//! their waypoints, queued moves, hunts, animals, fog exploration and last sightings.
//! Route searches, trade runs, carcasses and a General's current decisions are
//! restarted. `RestoreReport` counts misses.
mod general;
mod orders;
mod rate;
mod records;
use super::{BuildingKind, ConstructionState, GameUnit, GameWorld, GameWorldError};
use aoe_core::{EntityId, REFERENCE_HZ, TileCoord, TileRect, WorldPosition};

/// How far from its place a restored unit may be put when that tile is taken.
const NEAREST_RADIUS: i32 = 12;
pub use records::*;
use std::collections::BTreeMap;

impl GameWorld {
    /// The resumable state of this game; `content_hash` binds the resource
    /// changes to the map package.
    pub fn snapshot(&self, content_hash: [u8; 32]) -> Option<WorldSnapshot> {
        let resources = self.resource_snapshot(content_hash);
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
                transform_until: self
                    .combat
                    .trebuchets_transforming
                    .get(&stored.state.id)
                    .copied(),
                swing_started: if crate::native_attack_windup_millis(stored.state.kind).is_some() {
                    self.native_attack_started(stored.state.id).map(|t| t.0)
                } else {
                    None
                },
                scalar_attack_move: self.scalar_march_destination(stored.state.id),
                patrol: self.combat.patrols.get(&stored.state.id).copied(),
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
                queue: self.snapshot_pending_moves(stored.state.id),
                hunt: self.hunting_target(stored.state.id),
                garrison_order: self.economy.garrison_orders.get(&stored.state.id).copied(),
                shelter_intent: self
                    .economy
                    .shelter_intents
                    .get(&stored.state.id)
                    .copied()
                    .unwrap_or_default(),
                embarked_in: self.economy.embarked.get(&stored.state.id).copied(),
                general_controller: self.general_controller(stored.state.id),
                general_order_tick: self.economy.generals.manual.get(&stored.state.id).copied(),
                management: self.management_of(stored.state.id, stored.state.kind),
            })
            .collect();
        for building in self.economy.buildings.values() {
            for held in &building.garrison {
                units.push(UnitRecord {
                    unit: held.unit,
                    next_hit_tick: held.next_hit_tick,
                    transform_until: None,
                    swing_started: None,
                    scalar_attack_move: None,
                    patrol: None,
                    gather: None,
                    build: None,
                    garrison_in: Some(building.id),
                    move_to: None,
                    queue: Vec::new(),
                    hunt: None,
                    garrison_order: None,
                    shelter_intent: held.intent,
                    embarked_in: None,
                    general_controller: self.general_controller(held.unit.id),
                    general_order_tick: self.economy.generals.manual.get(&held.unit.id).copied(),
                    management: self.management_of(held.unit.id, held.unit.kind),
                });
            }
        }
        units.sort_by_key(|record| record.unit.id);
        let buildings = self
            .economy
            .buildings
            .values()
            .map(|b| {
                self.economy
                    .queues
                    .project(self.economy.construction.project(b))
            })
            .map(|b| {
                let mut record = BuildingRecord::from(b.as_ref());
                record.general_recruit = self.economy.generals.recruits.get(&b.id).copied();
                record
            })
            .collect();
        // A save holds reference ticks, so it resumes at any rate.
        let snapshot = WorldSnapshot {
            version: WORLD_SNAPSHOT_VERSION,
            tick: self.tick.0,
            next_entity_id: self.next_entity_id,
            fog_disabled: self.fog_disabled(),
            hp_quarters: self.combat.hp_quarters.clone(),
            players: self.economy.players.values().copied().collect(),
            market_recovery: self.economy.market_recovery.snapshot(self.tick.0),
            resources,
            buildings,
            dispatch_sites: self
                .economy
                .dispatch
                .sites
                .iter()
                .map(|(&site, &v)| (site, v))
                .collect(),
            dispatch_cursor: self.economy.dispatch.cursor,
            units,
            formations: self.formation_records(),
            animals: self.animal_records(),
            explored: self.explored_records(),
            building_memory: self.building_memory_records(),
            resource_memory: self.resource_memory_records(),
            town_bells: self.economy.town_bells.iter().copied().collect(),
        };
        Some(snapshot.retimed(self.config.tick_hz, REFERENCE_HZ))
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
        let snapshot = &snapshot.clone().retimed(REFERENCE_HZ, self.config.tick_hz);
        if let Some(resources) = &snapshot.resources {
            self.restore_resources(resources, content_hash, cancelled)
                .map_err(|error| SnapshotError::Map(error.to_string()))?;
        }
        // Sheltered units are briefly placed outside before re-entering their
        // building. Those temporary positions must not explore the new layout.
        // Rebuild sight once every entity has its final restored position.
        self.set_fog_disabled(true);
        for player in &snapshot.players {
            self.economy.players.insert(player.player, *player);
        }
        self.tick = aoe_core::Tick(snapshot.tick);
        self.economy
            .market_recovery
            .restore(snapshot.tick, &snapshot.market_recovery);
        self.economy.construction.tick = snapshot.tick;
        self.economy.queues.training.tick = snapshot.tick;
        self.economy.queues.research.tick = snapshot.tick;
        self.restore_explored(&snapshot.explored);
        let mut report = RestoreReport::default();
        // Identities are placed one at a time at their own number: buildings
        // first (a sheltered unit needs its building), then units.
        self.count_housing_once(|world| {
            for record in &snapshot.buildings {
                world.expect_id(record.id);
                world.restore_building(record, &mut report);
            }
        });
        for record in &snapshot.units {
            self.expect_id(record.unit.id);
            self.restore_unit(record, &mut report);
        }
        self.economy
            .town_bells
            .extend(snapshot.town_bells.iter().copied().filter(|id| {
                self.economy
                    .buildings
                    .get(id)
                    .is_some_and(|b| b.kind == BuildingKind::TownCenter && b.is_complete())
            }));
        for &(site, vassal) in &snapshot.dispatch_sites {
            if self
                .building(site)
                .is_some_and(|b| !b.is_complete() && b.hp > 0)
            {
                self.economy.dispatch.sites.insert(site, vassal);
            }
        }
        self.economy.dispatch.cursor = snapshot.dispatch_cursor;
        self.restore_general_ownership(&snapshot.units);
        for record in &snapshot.units {
            if let Some(transport) = record.embarked_in {
                self.restore_embarked(record.unit.id, transport);
            }
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
        if report.units_lost + report.buildings_lost > 0 {
            return Err(SnapshotError::Unplaced {
                units: report.units_lost,
                buildings: report.buildings_lost,
            });
        }
        self.restore_general_recruits(&snapshot.buildings, &snapshot.units);
        self.restore_building_memory(&snapshot.building_memory);
        self.restore_resource_memory(&snapshot.resource_memory);
        self.set_fog_disabled(snapshot.fog_disabled);
        self.resume_jobs(snapshot, &mut report);
        self.rebind_trade_units();
        for player in &snapshot.players {
            if let Some(slot) = self.economy.players.get_mut(&player.player) {
                *slot = *player;
            }
        }
        self.mark_all_buildings_changed();
        self.remember_visible_buildings();
        Ok(report)
    }

    /// The next allocation hands out `id`.
    fn expect_id(&mut self, id: EntityId) {
        self.next_entity_id = u64::from(id.0);
        self.combat.hp_quarters.resize(id.0 as usize, 0);
    }

    fn restore_building(&mut self, record: &BuildingRecord, report: &mut RestoreReport) {
        let complete = record.state == ConstructionState::Complete;
        let placed = self.insert_building_shape(
            record.player,
            record.kind,
            record.footprint,
            complete && record.kind != BuildingKind::Bridge,
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
        if record.state != ConstructionState::Planned && self.start_foundation(id).is_err() {
            self.remove_building(id);
            if record.kind == BuildingKind::Bridge && complete {
                report.bridges_dropped += 1;
                return;
            }
            report.buildings_lost += 1;
            return;
        }
        let Some(building) = self.building_mut(id) else {
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
        building.gate_locked = record.gate_locked;
        building.rally_point = record.rally_point;
        building.farm_reseeds = record.farm_reseeds;
        if !record.training_queue.is_empty() {
            self.economy.active_training.insert(id);
            self.economy.queues.training.awake.insert(id);
            self.economy.queues.register_training(id, record.player);
        }
        if record.research.is_some() {
            self.economy.active_research.insert(id);
            self.economy.queues.research.awake.insert(id);
        }
        if complete && record.kind.is_gate() {
            self.restore_gate(id, record.gate_open);
        }
        if complete && let Some(span) = record.bridge_span {
            if let Some(bridge) = self.building_mut(id) {
                bridge.state = ConstructionState::Complete;
            }
            self.unindex_foundation(id, record.player, record.footprint);
            self.restore_bridge_deck(span);
        }
        report.buildings += 1;
    }

    fn restore_unit(&mut self, record: &UnitRecord, report: &mut RestoreReport) {
        let state = record.unit;
        let spawned = self.place_restored(&state, record.garrison_in);
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
        unit.sergeant = state.sergeant;
        unit.last_hit = state.last_hit;
        if state.kind.is_trebuchet()
            && let Some(until) = record.transform_until.filter(|&until| until > self.tick.0)
        {
            self.combat.trebuchets_transforming.insert(id, until);
        }
        if let Some(building) = record.garrison_in
            && !self.shelter_restored(id, building, record.shelter_intent)
        {
            report.units_lost += 1;
            return;
        }
        report.units += 1;
    }

    /// Spawns a unit at its saved place, or at the nearest free tile (around
    /// its shelter for a sheltered unit, whose saved place is meaningless).
    fn place_restored(
        &mut self,
        state: &GameUnit,
        shelter: Option<EntityId>,
    ) -> Result<EntityId, GameWorldError> {
        let around = match shelter.and_then(|id| self.building(id)) {
            Some(building) => building.footprint,
            None => {
                if shelter.is_none() {
                    let first = self.spawn_kind(state.player, state.kind, state.position);
                    if first.is_ok() {
                        return first;
                    }
                }
                let tile = state.position.tile_floor();
                TileRect::new(tile, TileCoord::new(tile.x + 1, tile.y + 1))
            }
        };
        for radius in 0..=NEAREST_RADIUS {
            let (min, max) = (
                TileCoord::new(around.min.x - radius, around.min.y - radius),
                TileCoord::new(around.max.x + radius, around.max.y + radius),
            );
            for x in min.x..max.x {
                for y in min.y..max.y {
                    let edge = x < around.min.x - radius + 1
                        || x >= max.x - 1
                        || y < around.min.y - radius + 1
                        || y >= max.y - 1;
                    let tile = TileCoord::new(x, y);
                    if (!edge && radius > 0) || around.contains(tile) {
                        continue;
                    }
                    let Ok(at) = WorldPosition::from_tile_center(tile) else {
                        continue;
                    };
                    if let Ok(id) = self.spawn_kind(state.player, state.kind, at) {
                        return Ok(id);
                    }
                }
            }
        }
        Err(GameWorldError::InvalidPosition)
    }

    /// Puts management settings back; the unit's own activation (sheltering in
    /// its Town Center or Castle) has already happened.
    fn restore_management(&mut self, record: &UnitRecord) -> bool {
        let (id, player) = (record.unit.id, record.unit.player);
        match record.management {
            Some(Management::Steward(settings)) => {
                self.issue_steward_settings(player, id, settings).is_ok()
            }
            Some(Management::Vassal { development, focus }) => self
                .issue_vassal_focus(player, id, focus)
                .and_then(|()| self.issue_vassal_development(player, id, development))
                .is_ok(),
            Some(Management::General {
                training,
                mix,
                solo,
                destination,
                owner_march,
            }) => {
                let Some(army) = self.economy.generals.armies.get_mut(&id) else {
                    return false;
                };
                army.training = training;
                army.mix = mix;
                army.solo = solo;
                if let Some(destination) =
                    destination.filter(|&at| self.config.valid_ground_position(at))
                {
                    army.destination = Some(destination);
                    army.owner_march = if solo { None } else { owner_march };
                    army.state = crate::GeneralState::Marching;
                    army.routed = None;
                }
                true
            }
            None => false,
        }
    }
}
