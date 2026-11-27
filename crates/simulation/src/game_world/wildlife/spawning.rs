use super::{AnimalKind, AnimalTask, GameAnimal, HERD_RECRUIT_ID_START, StoredAnimal, cell};
use crate::game_world::snapshot::AnimalRecord;
use crate::{EconomyError, GameWorld};
use aoe_core::{EntityId, TileCoord, WorldPosition};

impl GameWorld {
    /// Living animals as a save keeps them (carcasses are not kept).
    pub(crate) fn animal_records(&self) -> Vec<AnimalRecord> {
        self.animal_records_of(|_| true)
    }

    pub(super) fn animal_records_of(
        &self,
        include: impl Fn(AnimalKind) -> bool,
    ) -> Vec<AnimalRecord> {
        self.wildlife
            .animals
            .values()
            .filter(|a| a.state.hp > 0 && a.state.death_tick.is_none() && include(a.state.kind))
            .map(|a| AnimalRecord {
                id: a.state.id,
                kind: a.state.kind,
                owner: a.state.owner,
                hp: a.state.hp,
                hp_quarters: a.hp_quarters,
                position: a.state.position,
                food_remaining: a.state.food_remaining,
                facing: a.state.facing,
                home: a.home,
            })
            .collect()
    }

    /// Puts a saved animal back at its own identity (the next allocation).
    pub(crate) fn restore_animal(&mut self, record: &AnimalRecord) -> bool {
        self.place_saved_animal(record, None) == Some(record.id)
    }

    pub(super) fn restore_map_animal(&mut self, record: &AnimalRecord) -> Option<EntityId> {
        // The starting kit reserved its ordinary ids before being removed.
        // Reuse those slots. Allocate a compact id for later ordinary animals
        // or when the recreated village occupies the old slot.
        let reserved = (u64::from(record.id.0) < self.next_entity_id
            && !self.lookup.contains_key(&record.id)
            && self.building(record.id).is_none()
            && self.animal(record.id).is_none())
        .then_some(record.id);
        let id = self.place_saved_animal(record, reserved)?;
        if let Some(owner) = record.owner {
            let tile = self.animal(id)?.position.tile_floor();
            self.update_observer(id, Some((owner, tile, record.kind.rules().los_tiles)));
        }
        Some(id)
    }

    fn place_saved_animal(
        &mut self,
        record: &AnimalRecord,
        reserved: Option<EntityId>,
    ) -> Option<EntityId> {
        // The saved game held these animals already: its limit is not applied again.
        let limit = std::mem::replace(&mut self.economy.limits.animals, usize::MAX);
        let recruit = record.id.0 >= HERD_RECRUIT_ID_START;
        if recruit {
            self.wildlife.next_herd_recruit = record.id.0 - HERD_RECRUIT_ID_START;
        }
        let mut spawned = self.spawn_animal_in(record.kind, record.position, recruit, reserved);
        // A foundation laid over it since: the nearest open tile, ring by ring.
        let home = record.position.tile_floor();
        for radius in 1..=12_i32 {
            if spawned.is_ok() {
                break;
            }
            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    if spawned.is_ok() || (dx.abs() != radius && dy.abs() != radius) {
                        continue;
                    }
                    let tile = aoe_core::TileCoord::new(home.x + dx, home.y + dy);
                    if let Ok(at) = WorldPosition::from_tile_center(tile) {
                        spawned = self.spawn_animal_in(record.kind, at, recruit, reserved);
                    }
                }
            }
        }
        self.economy.limits.animals = limit;
        let id = spawned.ok()?;
        if let Some(animal) = self.wildlife.animals.get_mut(&id) {
            animal.state.owner = record.owner;
            animal.state.hp = record.hp;
            animal.hp_quarters = record.hp_quarters;
            animal.state.food_remaining = record.food_remaining;
            animal.state.facing = record.facing;
            animal.home = record.home;
        }
        Some(id)
    }
    pub fn spawn_animal(
        &mut self,
        kind: AnimalKind,
        at: WorldPosition,
    ) -> Result<EntityId, EconomyError> {
        self.spawn_animal_in(kind, at, false, None)
    }

    pub(super) fn spawn_herd_recruit(
        &mut self,
        kind: AnimalKind,
        at: WorldPosition,
    ) -> Result<EntityId, EconomyError> {
        self.spawn_animal_in(kind, at, true, None)
    }

    pub(crate) fn animal_hp_quarters_mut(&mut self, id: EntityId) -> Option<&mut u8> {
        self.wildlife
            .animals
            .get_mut(&id)
            .map(|a| &mut a.hp_quarters)
    }

    fn spawn_animal_in(
        &mut self,
        kind: AnimalKind,
        at: WorldPosition,
        recruit: bool,
        reserved: Option<EntityId>,
    ) -> Result<EntityId, EconomyError> {
        if self.wildlife.animals.len() >= self.economy.limits.animals {
            return Err(EconomyError::Capacity);
        }
        if !self.config.valid_ground_position(at)
            || !self.tile_passable(at.tile_floor())
            || (kind == AnimalKind::Wolf && !self.animal_spawn_open(at.tile_floor()))
        {
            return Err(crate::GameWorldError::InvalidPosition.into());
        }
        let id = if let Some(id) = reserved {
            id
        } else if recruit {
            let id = HERD_RECRUIT_ID_START
                .checked_add(self.wildlife.next_herd_recruit)
                .filter(|&id| id < u32::MAX)
                .ok_or(crate::GameWorldError::EntityIdExhausted)?;
            self.wildlife.next_herd_recruit += 1;
            EntityId(id)
        } else {
            self.allocate_entity_id()?
        };
        self.wildlife.animals.insert(
            id,
            StoredAnimal {
                state: GameAnimal {
                    id,
                    kind,
                    owner: None,
                    hp: kind.rules().hp,
                    position: at,
                    task: AnimalTask::Idle,
                    food_remaining: kind.rules().food,
                    target: None,
                    facing: crate::Facing::South,
                    moving: false,
                    death_tick: None,
                },
                home: at,
                hp_quarters: 0,
                cooldown: 0,
                speed_carry: 0,
                decay_carry: 0,
                herd: None,
            },
        );
        self.wildlife.cells.entry(cell(at)).or_default().insert(id);
        Ok(id)
    }

    /// Drop the animals of one kind from the village kit and their indices.
    pub(crate) fn remove_animals(&mut self, kind: AnimalKind) {
        let found: Vec<_> = self
            .wildlife
            .animals
            .iter()
            .filter(|(_, a)| a.state.kind == kind)
            .map(|(id, a)| (*id, cell(a.state.position)))
            .collect();
        for (id, key) in found {
            self.herd_lost_member(id);
            self.wildlife.animals.remove(&id);
            self.wildlife.engaged.remove(&id);
            self.update_observer(id, None);
            self.unindex_animal(id, key);
        }
    }

    /// The terrain generator has no animal hooks. A bounded local kit is
    /// placed after the resource kit without changing terrain or collision.
    /// Returns how many guaranteed boar/deer herds (food the village is owed)
    /// could not be placed in full, so the caller can report a partial start.
    pub(crate) fn initialize_wildlife(&mut self, center: TileCoord, root: TileCoord) -> usize {
        let mut pending = Vec::new();
        let mut short_herds = 0;
        let region = self.animal_start_region(center, root);
        let mut used = std::collections::BTreeSet::new();
        let extra_boar = self.start_roll(center, 200).is_multiple_of(2);
        for (slot, (kind, founders, recruits, min, max)) in [
            (
                AnimalKind::Sheep,
                if extra_boar { 7 } else { 8 },
                0,
                3_u64,
                6_u64,
            ),
            (AnimalKind::Boar, if extra_boar { 3 } else { 2 }, 1, 9, 15),
            (AnimalKind::Deer, if extra_boar { 2 } else { 4 }, 2, 9, 15),
            (AnimalKind::Wolf, 1, 0, 16, 20),
        ]
        .into_iter()
        .enumerate()
        {
            let distance = |t: TileCoord, c: TileCoord| {
                u64::from(t.x.abs_diff(c.x)).pow(2) + u64::from(t.y.abs_diff(c.y)).pow(2)
            };
            let mut candidates: Vec<_> = region
                .iter()
                .copied()
                .filter(|tile| {
                    (min.pow(2)..=max.pow(2)).contains(&distance(*tile, center))
                        && self.tile_free(*tile)
                        && (kind != AnimalKind::Wolf || self.animal_spawn_open(*tile))
                })
                .collect();
            let salt = self.start_roll(center, 210 + slot as u64);
            candidates.sort_by_key(|tile| self.start_roll(*tile, salt));
            let guaranteed = matches!(kind, AnimalKind::Boar | AnimalKind::Deer);
            let Some(anchor) = candidates.first().copied() else {
                short_herds += usize::from(guaranteed);
                continue;
            };
            candidates.sort_by_key(|tile| {
                distance(*tile, anchor) * 8 + self.start_roll(*tile, salt) % 64
            });
            let mut members = Vec::new();
            for tile in candidates
                .iter()
                .copied()
                .filter(|tile| !used.contains(tile))
                .take(founders)
                .collect::<Vec<_>>()
            {
                if let Ok(at) = WorldPosition::from_tile_center(tile)
                    && let Ok(id) = self.spawn_animal(kind, at)
                {
                    used.insert(tile);
                    members.push(id);
                }
            }
            if guaranteed {
                pending.push((kind, founders + recruits, members, candidates));
            } else {
                self.register_herd(kind, &members);
            }
        }
        // Place the core wildlife of each village first. The additional game
        // can only use ground that those animals did not already claim.
        for (kind, target, mut members, candidates) in pending {
            for tile in candidates {
                if members.len() == target {
                    break;
                }
                if !used.contains(&tile)
                    && !self.living_animal_at(tile)
                    && let Ok(at) = WorldPosition::from_tile_center(tile)
                    && let Ok(id) = self.spawn_herd_recruit(kind, at)
                {
                    used.insert(tile);
                    members.push(id);
                }
            }
            short_herds += usize::from(members.len() < target);
            self.register_herd(kind, &members);
        }
        short_herds
    }
}
