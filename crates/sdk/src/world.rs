//! The slot's world model, built only from what the server sends it.
use crate::{
    catalog::ResourceCatalog,
    events::Event,
    fog::FogMemory,
    model::{Animal, Projectile, UnitTask},
};
use aoe_core::{EntityId, PlayerId};
use aoe_protocol::{
    GameplayBuildingState, GameplayPlayerState, GameplayServerMessage, GameplayUnitState, Tech,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

/// Bound on remembered dying units (an event dedup set, not game state).
const MAX_DYING_MEMORY: usize = 8_192;

/// A snapshot of everything one slot knows. Cloning is cheap: the fog memory,
/// the resource catalog and resource amounts are shared until written.
#[derive(Clone, Debug)]
pub struct World {
    pub(crate) world_id: u64,
    pub(crate) tick: u64,
    pub(crate) tick_hz: u32,
    pub(crate) width: i32,
    pub(crate) height: i32,
    pub(crate) map_hash: Option<[u8; 32]>,
    pub(crate) me: Option<PlayerId>,
    pub(crate) primary: EntityId,
    /// The subscription revision this model accepts messages for.
    pub(crate) subscription: u64,
    pub(crate) units: BTreeMap<EntityId, GameplayUnitState>,
    pub(crate) buildings: BTreeMap<EntityId, GameplayBuildingState>,
    pub(crate) animals: BTreeMap<EntityId, Animal>,
    pub(crate) projectiles: Vec<Projectile>,
    pub(crate) economy: Option<GameplayPlayerState>,
    pub(crate) fog: Arc<FogMemory>,
    pub(crate) catalog: Arc<ResourceCatalog>,
    pub(crate) amounts: Arc<BTreeMap<u64, u16>>,
    pub(crate) fog_dirty: bool,
    pub(crate) snapshot_seen: bool,
    dying_seen: BTreeSet<EntityId>,
}

impl World {
    /// A model for the match described by the server's `Welcome`.
    pub(crate) fn from_welcome(message: &GameplayServerMessage) -> Option<Self> {
        let GameplayServerMessage::Welcome {
            world_id,
            map_content_hash,
            width_tiles,
            height_tiles,
            tick_hz,
            primary_unit_id,
            ..
        } = message
        else {
            return None;
        };
        Some(Self {
            world_id: *world_id,
            tick: 0,
            tick_hz: *tick_hz,
            width: *width_tiles,
            height: *height_tiles,
            map_hash: *map_content_hash,
            me: None,
            primary: *primary_unit_id,
            subscription: 0,
            units: BTreeMap::new(),
            buildings: BTreeMap::new(),
            animals: BTreeMap::new(),
            projectiles: Vec::new(),
            economy: None,
            fog: Arc::new(FogMemory::default()),
            catalog: Arc::new(ResourceCatalog::default()),
            amounts: Arc::new(BTreeMap::new()),
            fog_dirty: false,
            snapshot_seen: false,
            dying_seen: BTreeSet::new(),
        })
    }

    /// Folds one server message into the model and reports what happened.
    pub(crate) fn apply(&mut self, message: GameplayServerMessage) -> Vec<Event> {
        use GameplayServerMessage as M;
        let mut events = Vec::new();
        match message {
            M::Snapshot {
                revision,
                tick,
                units,
                players,
                buildings,
            } if revision == self.subscription => {
                self.tick = tick.0;
                self.units = units.into_iter().map(|u| (u.id, u)).collect();
                self.buildings = buildings.into_iter().map(|b| (b.id, b)).collect();
                self.dying_seen = self
                    .units
                    .values()
                    .filter(|u| dying(u))
                    .map(|u| u.id)
                    .collect();
                if self.me.is_none() {
                    self.me = self.identify(&players);
                }
                self.economy = players.into_iter().find(|p| Some(p.player) == self.me);
                self.snapshot_seen = true;
            }
            M::Tick {
                revision,
                tick,
                changed_units,
                removals,
                changed_players,
                changed_buildings,
                building_removals,
            } if revision == self.subscription => {
                self.tick = tick.0;
                for unit in changed_units {
                    self.apply_unit(unit, &mut events);
                }
                for id in removals {
                    self.units.remove(&id);
                    self.dying_seen.remove(&id);
                }
                for player in changed_players {
                    self.apply_player(player, &mut events);
                }
                for building in changed_buildings {
                    self.apply_building(building, &mut events);
                }
                for id in building_removals {
                    if let Some(old) = self.buildings.remove(&id) {
                        events.push(Event::BuildingDestroyed {
                            id,
                            kind: old.kind,
                            owner: old.player,
                            mine: Some(old.player) == self.me,
                        });
                    }
                }
            }
            M::WildlifeState {
                revision, animals, ..
            } if revision == self.subscription => {
                self.animals = animals.into_iter().map(|a| (a.id, a)).collect();
            }
            M::Projectiles {
                revision,
                projectiles,
                ..
            } if revision == self.subscription => self.projectiles = projectiles,
            M::FogState(fog) if fog.revision == self.subscription => {
                let size = (self.width, self.height);
                Arc::make_mut(&mut self.fog).apply(&fog, size);
                self.fog_dirty = true;
            }
            M::ResourceState(state) if state.subscription_revision == self.subscription => {
                let amounts = Arc::make_mut(&mut self.amounts);
                if state.from_revision.is_none() {
                    amounts.clear();
                }
                for change in state.changes {
                    amounts.insert(change.id, change.remaining);
                }
            }
            M::CommandAck {
                sequence, result, ..
            } => events.push(Event::OrderResult { sequence, result }),
            M::RoleChange {
                primary_unit_id, ..
            } => self.primary = primary_unit_id,
            M::WorldReset { .. } => events.push(Event::WorldReset),
            M::Error { code, message } => events.push(Event::ServerError { code, message }),
            _ => {}
        }
        events
    }

    /// Which player is this slot? A fogged snapshot lists only its own
    /// economy; otherwise the owner of the primary unit.
    fn identify(&self, players: &[GameplayPlayerState]) -> Option<PlayerId> {
        match players {
            [only] => Some(only.player),
            _ => self.units.get(&self.primary).map(|unit| unit.player),
        }
    }

    fn apply_unit(&mut self, unit: GameplayUnitState, events: &mut Vec<Event>) {
        let mine = Some(unit.player) == self.me;
        let old = self.units.insert(unit.id, unit);
        if mine {
            if old.is_none() && !dying(&unit) {
                events.push(Event::UnitSpawned {
                    id: unit.id,
                    kind: unit.kind,
                });
            }
            if let Some(hit) = unit.last_hit
                && old.and_then(|o| o.last_hit) != Some(hit)
            {
                events.push(Event::UnderAttack {
                    target: unit.id,
                    attacker: Some(hit.attacker),
                    building: false,
                    tile: unit.position.tile_floor(),
                });
            }
        }
        if dying(&unit) && self.dying_seen.insert(unit.id) {
            if self.dying_seen.len() > MAX_DYING_MEMORY {
                self.dying_seen.clear();
                self.dying_seen.insert(unit.id);
            }
            events.push(Event::UnitDied {
                id: unit.id,
                kind: unit.kind,
                owner: unit.player,
                mine,
                tile: unit.position.tile_floor(),
            });
        }
    }

    fn apply_building(&mut self, building: GameplayBuildingState, events: &mut Vec<Event>) {
        let mine = Some(building.player) == self.me;
        let old = self.buildings.insert(building.id, building.clone());
        if !mine {
            return;
        }
        let complete =
            |b: &GameplayBuildingState| b.construction == aoe_protocol::ConstructionState::Complete;
        if complete(&building) && old.as_ref().is_some_and(|o| !complete(o)) {
            events.push(Event::BuildingCompleted {
                id: building.id,
                kind: building.kind,
            });
        }
        if let Some(hit) = building.last_hit
            && old.as_ref().and_then(|o| o.last_hit) != Some(hit)
        {
            let f = building.footprint;
            events.push(Event::UnderAttack {
                target: building.id,
                attacker: Some(hit.attacker),
                building: true,
                tile: aoe_core::TileCoord::new((f.min.x + f.max.x) / 2, (f.min.y + f.max.y) / 2),
            });
        }
    }

    fn apply_player(&mut self, player: GameplayPlayerState, events: &mut Vec<Event>) {
        if Some(player.player) != self.me {
            return;
        }
        if let Some(old) = self.economy {
            for tech in Tech::ALL {
                if player.researched & tech.bit() != 0 && old.researched & tech.bit() == 0 {
                    events.push(Event::ResearchCompleted { tech });
                }
            }
            if player.age != old.age {
                events.push(Event::AgeAdvanced { age: player.age });
            }
        }
        self.economy = Some(player);
    }
}

fn dying(unit: &GameplayUnitState) -> bool {
    unit.task == UnitTask::Dying || unit.death_tick.is_some()
}
